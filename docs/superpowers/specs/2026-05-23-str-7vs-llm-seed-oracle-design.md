# str-7vs: LLM Seed Oracle — Design Spec

**Date:** 2026-05-23  
**Issue:** str-7vs — Optional LLM integration for seed generation and stuck-state recovery  
**Status:** Approved for implementation

---

## Overview

Add an optional LLM oracle to shatter's concolic engine. When enabled, the oracle runs as a
continuous background producer: for each unsolved branch condition in the current exploration,
it fires a targeted LLM request (one condition per call) and feeds the resulting candidate
inputs into the orchestrator's selection loop. The LLM is not triggered by stuck-state
detection — it runs unconditionally whenever it is enabled, bounded only by per-function
query budgets and a cumulative token cap.

---

## Crate Layout

```
shatter-core     defines SeedOracle trait, OracleContext, OracleSlotMap,
                 orchestrator integration

shatter-llm      new crate; depends on shatter-core (trait only)
  src/
    lib.rs
    prompt.rs    shared prompt construction and output parsing
    parse.rs     JSON extraction, type validation, deduplication
    mock.rs      MockSeedOracle (trait-level test double, no HTTP)
    (str-0o8)    openai.rs
    (str-9w8)    anthropic.rs
    (str-w4c)    google.rs
    (str-g5b)    custom.rs / local.rs

shatter-cli      depends on shatter-llm; constructs Box<dyn SeedOracle>,
                 passes into orchestrator at startup
```

Dependency direction: `shatter-cli → shatter-llm → shatter-core` (trait only).
`shatter-core` never imports HTTP or provider code.

---

## SeedOracle Trait & OracleContext

Defined in `shatter-core/src/oracle.rs`.

```rust
pub struct OracleContext {
    pub function_source: String,           // source text around target function (capped to context_lines)
    pub param_types:     Vec<ParamType>,   // typed parameter descriptors
    pub condition:       FailedCondition,  // the one unsolved branch predicate
    pub attempted:       Vec<InputVector>, // representative inputs that failed (capped at 5, oldest dropped first)
}

pub struct OracleResponse {
    pub candidates: Vec<InputVector>,      // raw LLM suggestions, pre-validation
    pub tokens_used: u32,                  // input + output tokens for this call
}

#[async_trait]
pub trait SeedOracle: Send + Sync {
    /// Fire one targeted request for a single unsolved condition.
    /// Implementations handle timeout, retry (up to max_retries), and
    /// return Err on hard failure.
    async fn query(&self, ctx: OracleContext) -> anyhow::Result<OracleResponse>;
}
```

`FailedCondition` is an existing (or thin newtype) struct: the branch predicate text the
solver could not negate, plus branch location. `InputVector` is the existing `Vec<Value>` alias.
`ConditionId` is a stable hash or index derived from `FailedCondition` used as the slot map key;
its exact form is an implementation detail determined during coding.

---

## Per-Condition Slot Machinery

Defined in `shatter-core/src/oracle.rs`.

```rust
pub enum OracleSlot {
    /// No request in flight. Will fire on next orchestrator tick if condition still unsolved.
    Idle,
    /// Request dispatched; handle yields OracleResponse when complete.
    Pending(tokio::task::JoinHandle<anyhow::Result<OracleResponse>>),
    /// Candidates ready to drain.
    Ready(VecDeque<InputVector>),
    /// Hard failure or budget exhausted. Will not be retried this run.
    Exhausted,
}

pub struct OracleSlotMap {
    oracle:        Arc<dyn SeedOracle>,
    slots:         HashMap<ConditionId, OracleSlot>,
    config:        LlmConfig,
    query_count:   u32,    // queries fired this function (vs max_queries_per_function)
    tokens_used:   u32,    // cumulative tokens this run (vs max_token_budget)
    semaphore:     Arc<tokio::sync::Semaphore>, // caps max_concurrent_requests
}

impl OracleSlotMap {
    /// Called by orchestrator on each unsolved condition each tick.
    /// - Idle + under budget + permit available → spawn request, transition to Pending
    /// - Pending → poll handle (non-blocking); on completion → Ready or Exhausted
    /// - Ready → pop one candidate, return it
    /// - Exhausted → return None
    pub fn poll(&mut self, id: ConditionId, ctx: OracleContext) -> Option<InputVector>;

    /// Called when a condition is solved. Aborts any Pending handle, removes slot.
    pub fn retire(&mut self, id: ConditionId);

    /// Returns counts for the post-run summary line.
    pub fn stats(&self) -> OracleStats; // { queries_fired, tokens_used, candidates_accepted }
}
```

The orchestrator calls `poll()` synchronously — it never awaits. `tokio::spawn` puts the
LLM request on the async runtime in the background; `JoinHandle::try_join` (non-blocking)
checks completion. A tokio runtime is started once at CLI startup and shared via
`Arc<Runtime>`.

---

## Orchestrator Integration

The oracle slots sit between genetic and random in the input-selection priority order:

```
1. User-provided inputs
2. Concolic solver (Z3)
3. Genetic / boundary search
4. LLM oracle slots          ← new; present only when --llm is active
5. Random fallback
```

On each round, for every unsolved `FailedCondition`:

1. **Build `OracleContext`** — function source (capped to `context_lines`), param types,
   the condition, and the 5 most-recently-tried inputs that failed to satisfy it.
2. **Call `slot_map.poll(id, ctx)`** — returns a candidate if `Ready`, fires a background
   request if `Idle` (and under budget), or returns `None` if `Pending`/`Exhausted`.
3. **Validate the candidate** — type-check each value against param types; deduplicate
   against already-tried inputs, behavior map cache, and current candidate pool. Drop invalids.
4. **On execution** — if the candidate reaches a new equivalence class, persist to the
   behavior map with provenance tag `source: LlmOracle`. Call `slot_map.retire(id)`.
5. **If the candidate covers no new ground** — add to the attempted set for this condition;
   slot transitions back to `Idle` to fire another request next tick (subject to budget).

`OracleSlotMap` is constructed in `shatter-cli` when `--llm` is present, wrapped in
`Option<OracleSlotMap>`, and passed into the orchestrator. All slot-map code is gated on
`option.is_some()` — zero overhead when LLM is disabled.

---

## Rate Limiting & Cost Controls

**`max_concurrent_requests`** — `OracleSlotMap` holds an `Arc<Semaphore>`. A slot can only
transition `Idle → Pending` if it acquires a permit. The permit is released when the handle
completes (Ready or Exhausted). Prevents N unsolved conditions from firing N simultaneous
HTTP requests.

**`max_queries_per_function`** — hard cap on `query_count`. Once reached, no new
`Idle → Pending` transitions fire for this function.

**`max_token_budget`** — cumulative token cap across the entire run. `tokens_used` is
updated from `OracleResponse.tokens_used` when each handle completes. Once exceeded, no new
`Idle → Pending` transitions fire (in-flight requests are allowed to complete).

**Provider-side 429 handling** — a `RateLimitedOracle` wrapper in `shatter-llm` intercepts
rate-limit errors from any adapter, applies exponential backoff, and retries transparently
up to `max_retries`. All adapters inherit this by being wrapped at construction time in
`shatter-cli`.

---

## Config & CLI

### `.shatter/config.yaml`

```yaml
llm:
  enabled: false                    # master toggle; --llm flag overrides
  adapter: anthropic                # openai | anthropic | google | custom | local | mock
  candidates_per_query: 3           # inputs requested per LLM call
  max_queries_per_function: 10      # query budget cap per function
  max_concurrent_requests: 4        # semaphore cap on simultaneous in-flight calls
  max_token_budget: 50000           # cumulative token cap; halts new queries when exceeded
  max_tokens_per_query: 1024        # output token limit per request
  temperature: 0.7
  timeout_seconds: 30
  max_retries: 2
  context_lines: 50                 # source lines sent around the target function

  # adapter-specific blocks (owned by child issues):
  # openai:    { model, api_key_env, base_url }
  # anthropic: { model, api_key_env }
  # google:    { model, api_key_env }
  # custom:    { url, headers, auth, request_mapping, response_mapping }
  # local:     { command, model, port, startup_timeout_seconds }
```

### CLI — `Scan` variant

`LlmOverrides` is a flattened, boxed struct. Boxing keeps the `Scan` enum variant size
constant regardless of future growth, avoiding the clap stack-budget issue (see str-1414).

```rust
#[derive(Args, Default)]
pub struct LlmOverrides {
    /// Enable LLM oracle (reads config for adapter and settings)
    #[arg(long)]
    pub llm: bool,

    /// LLM adapter to use, overrides llm.adapter in config
    #[arg(long, value_name = "ADAPTER")]
    pub llm_adapter: Option<String>,

    /// Cumulative token cap, overrides llm.max_token_budget in config
    #[arg(long, value_name = "TOKENS")]
    pub llm_token_budget: Option<u32>,
}

// In Scan variant:
#[command(flatten)]
pub llm: Box<LlmOverrides>,
```

### Post-run summary

Emitted by the CLI layer after the orchestrator returns, only when `--llm` was active:

```
LLM oracle: 7 queries · 4,821 tokens · 3 candidates accepted  [budget: 4,821 / 50,000]
```

Token budget exhaustion does not surface as an error — it is informational only.

---

## Prompt Construction

Defined in `shatter-llm/src/prompt.rs`. Shared by all adapters.

```
You are a test input generator for a concolic execution engine.

## Function under test
<source lines around the function, capped to context_lines>

## Parameter types
<typed parameter list, e.g. "x: i32, s: String, flag: bool">

## Unsolved branch condition
The following branch has not been covered. Generate inputs that satisfy it:
<condition text, e.g. "s.len() > 4 && s.chars().next() == Some('@')">

## Inputs already attempted (did not satisfy the condition)
<JSON array of up to 5 representative InputVectors that failed>

## Instructions
Return ONLY a JSON array of <candidates_per_query> input objects.
Each object must have one key per parameter with a value of the correct type.
Do not include explanation or prose.

Example format:
[{"x": 42, "s": "hello@world", "flag": true}, …]
```

**Structured output:** Adapters that support it (Anthropic `tool_use`, OpenAI JSON mode)
use a schema derived from the param types. Adapters that don't support it parse the first
JSON array found in the text response.

---

## Output Parsing, Validation & Deduplication

Defined in `shatter-llm/src/parse.rs`. Shared by all adapters.

1. **Extract JSON array** — structured-output adapters decode directly; text adapters scan
   for the first valid `[…]`. On failure, retry with a simplified prompt (condition only,
   no attempted-inputs context) up to `max_retries`. All retries exhausted → `Exhausted`.

2. **Type validation** — each candidate is validated against param type descriptors. Type
   mismatches are dropped individually (no coercion). Remaining candidates proceed.

3. **Deduplication** — each candidate is checked against:
   - The `attempted` set for this condition
   - The behavior map cache (already-executed inputs)
   - The current orchestrator candidate pool
   
   Duplicates are dropped. If all candidates deduplicate to nothing, the slot logs a warning
   and transitions to `Exhausted` for this condition.

4. **Provenance** — `source: LlmOracle` is attached to each surviving candidate and travels
   with it through execution into the behavior map.

---

## Testing

### Unit tests (no network)

Use `MockSeedOracle` — a hand-rolled `SeedOracle` implementation in `shatter-llm/src/mock.rs`
with scripted responses. No zolem, no HTTP. Covers slot state transitions, dedup logic,
parse-failure paths, budget caps — anything testable at the trait boundary.

```rust
pub struct MockSeedOracle { /* scripted response map */ }

impl MockSeedOracle {
    pub fn scripted(entries: Vec<(ConditionMatcher, Vec<InputVector>)>) -> Self;
    pub fn always_fail() -> Self;
    pub fn always_empty() -> Self;
}
```

### Integration tests (adapter layer, `shatter-llm`)

Use `github.com/ketang/zolem` as an HTTP mock server. Test that a real adapter correctly
constructs requests, parses responses, handles 429s, and applies backoff. Zolem serves
scripted HTTP responses; the adapter's HTTP client hits it. No real API keys required.

### E2E tests (`shatter-core/tests/e2e_llm_oracle.rs`)

Also use zolem, exercising the full pipeline:

```
CLI → LlmOverrides → OracleSlotMap → real adapter → zolem server
    → response parsed → candidate validated → orchestrator → behavior map
```

Adapters are pointed at the zolem server via `llm.<adapter>.base_url`. No `#[cfg(test)]`
code paths needed in adapter implementations.

**Required test cases before str-7vs closes:**

| Test | What it covers |
|---|---|
| First unsolved condition fires a request | `Idle → Pending → Ready` transition |
| Candidate accepted: new equivalence class reached | Provenance tag in behavior map |
| Candidate rejected: duplicate of attempted input | Dedup drops; slot retries |
| `max_queries_per_function` reached | No new slots fire after cap |
| `max_token_budget` reached | No new `Idle → Pending` after token cap |
| `max_concurrent_requests` semaphore | N+1 conditions fire only N concurrent requests |
| Parse failure + retry | `Exhausted` after `max_retries` failures |
| `--llm` absent | `Option<OracleSlotMap>` is `None`; no slot code runs |
| Post-run summary line | Correct token/query/accepted counts |
| `MockSeedOracle::always_fail` | `Exhausted` slot does not retry indefinitely |
| 429 rate-limit + backoff (integration) | `RateLimitedOracle` wrapper retries correctly |
| Full pipeline: LLM candidate covers new branch (E2E) | Zolem-backed end-to-end pass |

---

## Non-Goals

- LLM does not replace the concolic engine or Z3 solver
- LLM is never in the hot path — only invoked via background async tasks
- No LLM-based constraint solving (that is Z3's job)
- No per-query cost estimation (token counts are tracked; dollar conversion is not)
- Adapter implementations (OpenAI, Anthropic, Google, custom) are out of scope for str-7vs
  and covered by str-0o8, str-9w8, str-w4c, str-g5b

---

## Open Issues Filed During Design

- **str-1414** — Reexamine shatter-cli Cli enum for clap stack limitation; potential internal
  and/or visible-surface redesign with regression guard.
