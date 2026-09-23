# Area review: shatter-rust, shatter-rust-runtime, shatter-llm (2026-09-22)

Reviewer scope: L1 (code quality), L2 (docs vs code), L4 (design), with L5/AGENT
observations where evidence surfaced. Worktree: `audit-2026-09-22` @ `16794cef`.
Observation only; nothing was edited except this file.

## Method

- Read `shatter-rust/CLAUDE.md` (263 lines) in full, `lib.rs`, `main.rs`, the handler
  dispatch loop, `instrument.rs` constraint builder, the analyzer's crate type registry,
  the persistent-harness I/O path in `executor.rs`, the crate-bridge wrapper generator,
  `shatter-rust-runtime/src/lib.rs` public API, and all of `shatter-llm`'s registry,
  rate-limit, parse, Anthropic and Jev adapters.
- Cross-checked `protocol/parity-matrix.yaml` (rust entries), `PARITY.md`, root and
  per-crate Taskfiles, `scripts/affected-gates.py`, `.github/workflows/ci.yml`.
- Dedupe: live `bd` (not `.beads/issues.jsonl`, which lags the DB — e.g. jsonl shows
  str-qwua7.14 open while `bd show` says closed).
- Live probes: ran the existing `shatter-rust` debug binary (built 2026-09-21, newer than
  the last change to analyzer/instrument/handler on 2026-08-27) over the protocol, and the
  `shatter` debug CLI (built 2026-09-22 10:33) with `explore --concolic` on small probe
  crates in the session scratchpad. Machine load was 100-190 during the run, so one
  explore attempt timed out; I re-ran it with longer timeouts.
- `cargo test --no-run` for shatter-rust completed. Test-binary counts were verified with `--list`.

## Prior-audit status (2026-09-04 items in this area)

| Prior item | Status 2026-09-22 | Evidence |
|---|---|---|
| executor.rs 16k god file (str-26ky, qwua7 rec 2) | **Unchanged**: still 16,125 lines, byte-identical line count | `wc -l shatter-rust/src/executor.rs` |
| Hand-formatted runtime constraint JSON (str-qwua7.36) | **Still open, and worse than the issue says** (see F1, F2) | instrument.rs:519-712 |
| Poison-tolerant locks (str-qwua7.50) | Open; **the issue's premise is wrong** (see F5) | handler.rs:480-545 |
| Embed Rust frontend (str-qwua7.60) | Open, no code movement; `shatter-rust` still outside the workspace (`Cargo.toml` `exclude`) | root Cargo.toml:3 |
| core→shatter-llm dev-dep cycle (str-qwua7.43) | Open; **the cycle got deeper** after the issue was filed (see F9) | shatter-core/tests/bench_frontier_ranking.rs |
| Rust CLAUDE.md false "does not emit preflight_failed" (str-qwua7.34) | Still false: CLAUDE.md:163-173 vs handler.rs:341-380 | unchanged |
| Rust walkthrough 0% (str-qwua7.14) | Closed "not reproducible" | bd close reason |

## Findings

### F1 (P2, L1): hand-rolled constraint JSON is invalid for ordinary literals and fails silently

`instrument.rs:671-712` builds runtime constraints with `format!`. Two confirmed ways it
produces invalid JSON:

- Float literal with a trailing dot: `x > 1.` → `"value":1.` (`f.base10_digits()` returns `1.`).
- `escape_json_string` (:704-710) escapes only `\\ " \n \r \t`. Other control characters pass
  through raw, so `s == "a\u{1}b"` embeds U+0001 inside a JSON string.

The runtime's `branch_hit` (`shatter-rust-runtime/src/lib.rs:180-186`) then fails
`serde_json::from_str` and **silently** records `SymConstraint::Unknown { hint: <raw json> }`.
There is no warning, no counter, and no test.

Probe (instrument over protocol; then `shatter explore --concolic` on the same file):
```
branch_hit (0u32 , 2u32 , __shatter_cond , "{...\"value\":1.}}")
artifact unknown hints:
  {"kind":"bin_op","op":"gt",...,"right":{"kind":"const","type":"float","value":1.}}
  {"kind":"bin_op","op":"eq",...,"right":{"kind":"const","type":"str","value":"ab"}}  (U+0001 inside)
```
User impact: `ctrl(x,n,s)` with `if s == "a\u{1}b" {return 2}` never reached return 2 in 60
concolic iterations (paths: returns 4, 1, 3 only), yet the batch line printed `3/3 branches`.
In simple cases literal mining can hide the loss: `x > 1.` was still covered by a mined `2.0`.

The analyzer's typed builder handles both inputs correctly (`BR 2 ... "value": 1.0`,
`"value": "a\u0001b"`), so this is a second, weaker encoder of the same thing.
str-qwua7.36 only describes the operator gap. It does not name invalid-JSON output or the
silent fallback.

### F2 (P2, L1/L5): match-arm runtime constraints are string equality on the pattern's text

`instrument.rs:413-446` encodes every match arm as
`param(<scrutinee tokens>) == const str "<pattern tokens>"`. That covers int literals, ranges,
bindings, guards and `_`. It also leaves out the `path` field that every other param node emits.

Observed for `match n { 0=>.., 1..=5=>.., k if k>100=>.., _=>.. }` with `n: i64`:
```
{"kind":"param","name":"n"} == {"type":"str","value":"0"}
... == {"type":"str","value":"1 ..= 5"}
... == {"type":"str","value":"k"}
... == {"type":"str","value":"_"}
```
The analyzer has its own bug for binding-plus-guard arms: static condition for `k if k > 100` is
`n == "k"` (str). For `0` and `1..=5` it emits correct int constraints.

Coverage impact (explore --concolic, 60 iters, generous timeouts): function `clean`
(`if n + 1 > 5`, then the match above) reached only returns 4, 1, 2. It never reached 3, 5, 6 or 7,
and every execution used `n = 0`. Markdown said "3 path(s)" while the batch line said
"6 paths, 5/7 branches". A two-arm `match n {0,7,_}` was fully covered, but only because the
literals 0 and 7 are mined.

The Rust CLAUDE.md (:83) documents a shatter-core workaround: alphanumeric,
case-insensitive matching of `"t . purpose"` and `"TripPurpose :: Personal"`. The core is
compensating for frontend encoding debt.

### F3 (P1, L1/L5): crate-bridge harness shares stdout between user output and the protocol, so any function that prints fails as `internal_error`

When bin_only is `NonExecutable`, crate-bridge is the automatic fallback for files in a crate
(`executor.rs:6293-6329`). Its generated driver writes results with
`println!("{}", serde_json::to_string(&exec_result).unwrap())` (`executor.rs:5195`). It does not
redirect fds, while the standalone and dispatch harnesses do (`executor.rs:2524-2561`,
`2703-2721`). The reader takes the first stdout line as the response (`executor.rs:642-647`).

Probe (crate `bridgeprobe`, `harness_mode: crate_bridge`):
```
3 error internal_error output parse error: failed to parse execute result: expected value at line 1 column 1
  line: hello from user code 1      outcome=runtime_failed
4 execute ret=10 completed          (sibling quiet(5), after harness restart)
```
Any function that uses `println!` for logging or debugging is unexecutable in this mode. It is
also reported as an internal/runtime failure, not as a known limitation. CLAUDE.md:21 and the
parity matrix say only that crate-bridge "does not capture console output". Neither mentions
that printing breaks execution.

### F4 (P2, L4): timeout hierarchy is inverted, so a cold Rust harness build shows up as a generic request timeout

- CLI `--request-timeout` default is 30 s (`shatter-cli/src/args.rs:561-563`).
- CLI-governed `SHATTER_BUILD_TIMEOUT` is 30 s (PARITY.md:82). The frontend's own fallback is
  **120 s** (`executor.rs:1017`, raised by eeceb7b4 "allow cold Axum harness builds").
  PARITY.md:96 still says the Rust fallback is 30 s.
- First execute = cargo build + run, inside a single 30 s request.

Observed under load: `concolic observe failed: frontend error: request timed out after 30s` for
both functions, 0 iterations, at 32.7 s. The frontend's more specific build-timeout or
build_failed diagnostic can never reach the user because the request budget is ≤ the build
budget. str-qe9pp (prepare timeout in conformance) and str-35vtk.15 (cargo job budget) cover
neighbouring symptoms, not the budget relationship.

### F5 (P2, L4): str-qwua7.50 (poison-tolerant locks) fixes a failure mode that cannot happen, and misses the real one

`Handler::run` / `dispatch` (`handler.rs:480-545`) has no `catch_unwind`. Any panic in a request
ends the process (`main.rs` → exit 1), so a poisoned mutex is never observed by a later request.
qwua7.50's required acceptance test is "a request that panics while holding the lock, followed
by a normal request on the same handler, succeeds". That test cannot pass without first adding
a panic boundary.

The caches (`HarnessCache`, `CrateHarnessCache`, `CrateBridgeHarnessCache`, executor.rs:406, 526,
733) are only touched from the single dispatch thread. The spawned threads (executor.rs:3055,
3206, 5475) only forward child stdout. The Mutexes are unneeded synchronisation, and the real
robustness gap is "one panic kills the whole session".

### F6 (P2, L1/AGENT): every inline unit test runs twice, because `main.rs` re-declares the module tree

`main.rs:5-16` declares `mod adapters; mod analyzer; ... mod wasm_generator;` instead of using
the `shatter_rust` library, and adds `#![allow(dead_code)]`. Both the lib and bin unittest
targets therefore compile and run all 587 inline `#[test]`s. The prior gate log shows
`Starting 1180 tests across 3 binaries` (= 587 × 2 + 6 `codegen_parity`) and
`Summary [208.433s]` (`audits/2026-09-04/gates/gate-rust-fe-test.log:21-23`). Re-verified on
this tree after `cargo test --no-run`: `unittests src/lib.rs` has 587 tests, `unittests src/main.rs`
has 587 tests, and `tests/codegen_parity.rs` has 4 (`<bin> --list`). This suite is
already the slowest serialized step of `check-unit` (Taskfile.yml:556-562 explains why it is
kept sequential). Side effects: `ENV_LOCK` is duplicated (lib.rs:19 and main.rs:22 "mirrors
it"). Dead-code linting is also effectively off: the bin allows it, and the lib exports every
module `pub`.

### F7 (P2, AGENT): the gate cache and affected-selector miss files that change Rust-frontend test results

- `shatter-rust/Taskfile.yml` `test.sources` = `src/**/*.rs, Cargo.toml, Cargo.lock,
  ../.config/nextest-standalone.toml`. It omits `tests/**/*.rs` (codegen_parity.rs) and
  `../shatter-rust-runtime/**`. Many executor tests compile harnesses against the runtime
  located by `find_runtime_crate_path` (executor.rs:1198-1223). So a runtime-only change, or a
  change to `tests/codegen_parity.rs`, leaves `rust-fe:test` "up to date", and nothing runs.
- `scripts/affected-gates.py`: checked directly with `select_gates()`:
  - `shatter-rust-runtime/src/lib.rs` → `['smoke','rust-rt:clippy','rust-rt:test','e2e-rust']` (no `rust-fe:test`).
  - `shatter-llm/src/jev.rs` → `['smoke','check']`, and `check` never runs shatter-llm's tests or
    clippy (check-static/check-unit deps, Taskfile.yml:535-562). CI covers it only through two
    extra steps (ci.yml:90-103). So `task affected`, the completion-checklist gate, passes an
    llm change without ever running llm tests. str-35vtk.36 tracks folding llm into
    `task check`, but not the affected-selector hole.

### F8 (P2, L2): shatter-rust CLAUDE.md and the parity matrix are stale on several points not covered by str-qwua7.24/.25/.34

| Claim | Location | Reality |
|---|---|---|
| Tokio adapter wraps in `tokio::runtime::Runtime::new().unwrap().block_on(...)` | CLAUDE.md:109; parity-matrix `adapter_capabilities.async_runtime.notes` | All four generators use `Builder::new_current_thread()` (executor.rs:2547, 2845, 4942, 6801). The multi-thread default is deliberately avoided because runtime tracking is thread-local (executor.rs:6781-6791, str-oc67) |
| Cross-file enum resolution "out of scope (single-file constraint)"; Opaque reasons unprovable "single-file constraint" | CLAUDE.md:79, :93 | analyzer.rs:6-14 and :834-952 resolve same-crate cross-file structs/enums (str-do53). CLAUDE.md:86 contradicts :79 |
| `last_file` set at `handler.rs:552, 620`, fallback at line 803 | CLAUDE.md:234-235 | Actual 693, 767, 842/970 |
| Rust `console_output: captured` (status: required) | parity-matrix `side_effect_capabilities.console_output` | PARITY.md:37 says **P**; crate-bridge does not capture it and printing breaks execution (F3) |
| Axum extractor list | parity-matrix axum_handler.notes | Omits Multipart, which CLAUDE.md:110 and adapters.rs:410-457 support |
| Rust `SHATTER_BUILD_TIMEOUT` fallback 30 s | PARITY.md:96 | 120 s (executor.rs:1017) |
| `explore` help: "extension determines frontend (.ts = TypeScript, .go = Go)" | shatter-cli/src/args.rs:501 | `.rs` is supported but not named |

### F9 (P2, AGENT/L4): new work deepened the core→shatter-llm dev-dependency cycle that an open decided issue says to remove

str-qwua7.43 (filed 2026-09-05) says to move `e2e_llm_oracle.rs` out of core and drop
`shatter-llm` from core's dev-deps. The Jev benchmark plan (2026-09-21,
`docs/superpowers/plans/2026-09-21-jev-frontier-ranking-benchmark.md:1037`) explicitly **adds**
`shatter-llm` to core `[dev-dependencies]` for `shatter-core/tests/bench_frontier_ranking.rs`
(str-hjrnp.3, closed 2026-09-22). Core now has two llm-dependent tests. Nothing in the
planning or landing flow checked for open issues touching the same files or dependency edge.

### F10 (P2, tracker hygiene): duplicate and stale Rust-frontend issues

- str-1fik (P1, open, **empty body**) and str-wfd2 (P2, open) both request "cross-file/cross-crate
  struct synthesis". str-do53 (P1) closed 2026-07-07 after delivering **same-crate** resolution.
  The remaining work is cross-crate (dependency types, e.g. pickpackit's `pickpackit_api::domain`).
  No open issue is titled that way, and the P1 is a blank duplicate.
- str-uj3y (open) duplicates str-da35 (closed, landed). `build_frontend.rs:604-629` vendors the runtime.
- The auto-memory file `project_shatter_rust_single_file_analysis.md` (indexed in MEMORY.md)
  still says the analyzer has "no cross-file or cross-crate type resolution" and cites
  `convert_type_path` ~773-809. Agents are primed with a wrong model of the frontend.

### F11 (P2, L1): 38 tests silently pass when cargo cannot reach the network

`executor.rs:7611-7616` `is_offline_compile_error_message`. There are 15 match arms of
`Err(CompilationFailed(msg)) if is_offline_compile_error_message(&msg) => { eprintln!("skipping ..."); }`
and 38 "skipping" sites in executor.rs/handler.rs. Under nextest `status-level = "fail"`
(`.config/nextest-standalone.toml`), those messages are hidden, and the tests count as passed.
An offline or sandboxed run reports green with zero axum or native-replay coverage.

### F12 (P3, L4): crate type registry is rebuilt from scratch on every analyze

`analyze_file_with_timing` → `build_crate_type_registry(file_path)` (analyzer.rs:83, :201,
:907-952) walks and `syn::parse_file`s every `.rs` file in the crate on each call, with no
cache on the handler. Scanning N files in one crate costs O(N²) parses. Cache per
`(crate_root, max mtime)` on the Handler.

### F13 (P3, L1): on-disk harness cache keys use `DefaultHasher`

`source_hash`, `stable_crate_harness_dir`, `stable_crate_bridge_dir` (executor.rs:764-767, 839-847,
3234-3236, 3749) key **persistent** cache directories with `std::collections::hash_map::DefaultHasher`.
std documents its algorithm as unspecified across releases. The crate already uses SHA-256 for
`prepare_id` (executor.rs:4122). A toolchain bump invalidates caches silently, and 64-bit keys
are content addresses for binaries that get reused.

### F14 (P3, L4): two independent Axum extractor classifiers

`adapters.rs:389-470` `AxumExtractorKind` (12 kinds, keyed by `type_name`) and
`executor.rs:1796-1838` `AxumExtractor` (5 kinds, re-parsed with syn). The second one drives the
generic wrappers (executor.rs:2167-2236, 2452, 2776, 4989, 6571, 6693). This is the same
"parallel path" pattern root CLAUDE.md warns about.

### F15 (P3, L1): shatter-rust-runtime harness loop swallows malformed requests

`run_harness_loop` / `run_dispatch_loop` (runtime lib.rs:539-600):
`serde_json::from_str(line).unwrap_or_default()` turns a malformed request into `Null`, so the
handler runs with zero inputs and reports "input 0 deserialization failed". The
`flush_results` fallback (lib.rs:340-345) interpolates an error string into JSON without
escaping. Runtime state is thread-local (lib.rs:167), so branches executed on user-spawned
threads (std::thread, rayon) are silently dropped. CLAUDE.md does not document this.

### F16 (P3, L1, security hygiene): the Jev API key is reachable through derived `Debug`

`JevConfig { api_key }` derives `Debug` (shatter-llm/src/jev.rs:23). It is held by `JevAdapter`
(`#[derive(Debug)]`), then `ReplayDecisionOracle` (replay.rs:12), then `DecisionFrontierRanker`
(decision_ranker.rs:18), which sits in `ExploreConfig.frontier_ranker` (`#[derive(Debug, Clone)]`,
orchestrator.rs:102-164). No current log site prints it, but one `debug!("{config:?}")` would
leak `TYPESAFE_API_KEY`.

### F17 (P3, L1): shatter-llm validation and robustness gaps

- `parse.rs:103` `TypeInfo::Int { .. } => v.is_i64() || v.is_u64()` ignores
  `int_width`/`int_signed`, so an LLM candidate of `-1` for `u8` passes validation and fails in
  the frontend.
- `extract_first_json_array` (parse.rs:62-98) tries only the first `[`. Prose like
  "see f[0]: [...]" returns `None`.
- `rate_limit.rs:61`: `100u64 << attempt` has no cap. `max_retries` ≥ 64 panics in debug
  (shift overflow), and at 20 retries it sleeps about 29 h. `Retry-After` is not capped either.
- There are no property tests for the untrusted-output parser (formal-methods policy target).
  `jev.rs`/`replay.rs` have no inline tests; wiremock tests live in tests/jev_adapter.rs.

### F18 (P3, L3/L2): the LLM seed oracle has no user documentation

The only user-facing mention is SPEC.md:201 (one line naming `--llm`, `--llm-adapter`,
`--llm-token-budget`). The adapters, config keys (`llm.anthropic.api_key`,
`SHATTER_ANTHROPIC_API_KEY`, `llm.custom`, `llm.local`), costs and what leaves the machine are
documented only in design specs under docs/superpowers. Default model IDs are hard-coded
(`claude-sonnet-4-6`, `gpt-4o`, `gemini-2.0-flash`, shatter-core/src/config.rs:338-392), with no
update policy.

## shatter-llm: what it is, whether it is used, whether it is sound

- **What**: a plugin crate implementing `shatter_core::oracle::SeedOracle` (generative input
  suggestions from Anthropic/OpenAI/Google/custom-HTTP/local) plus, since 2026-09-22,
  `DecisionOracle` (Jev) + `DecisionFrontierRanker` + `ReplayDecisionOracle` for the
  frontier-ranking benchmark.
- **Used**: SeedOracle adapters are reachable from the CLI (`shatter-cli/src/helpers.rs:1605-1661`,
  `--llm` flags). Jev, the ranker and replay are **benchmark-only**, used from
  `shatter-core/tests/bench_frontier_ranking.rs` with no CLI wiring. That fits the str-hjrnp
  goal of measuring first.
- **Tested**: 71 unit tests + 11 integration tests (wiremock). They run in CI but not in
  `task check` or `task affected` (F7).
- **Design**: the separation is sound. Traits live in core, HTTP adapters in the plugin, and
  there is a rate-limit decorator and a registry. Weak points: an anyhow-typed public API, the
  dev-dependency cycle (F9), Debug derivations holding secrets (F16), and validation gaps (F17).

## Strengths worth keeping

- Non-test frontend code rarely panics. Harness failures map to typed `ExecuteError`, and those
  map to outcome statuses (CLAUDE.md "Outcome Emission Contract" matches handler.rs).
- The crate-bridge "no-poisoning" design (whole-file plan → single-function fallback, negative
  cache `POISONED_WHOLE_FILE_PLANS`, content-addressed dirs, cross-process build lock) is careful
  and well commented (executor.rs:528-545, 5745-5880).
- The same-crate type registry is conservative: ambiguous names are dropped, not guessed, and
  symlink loops and depth are bounded (analyzer.rs:864-952).
- The enum value-domain contract states a clear governing invariant ("every emitted member must
  be accepted by serde … never guess").
- `tests/codegen_parity.rs` pins the hand-rolled protocol constants to the codegen output.
- The runtime crate has a small, documented API, and its wire types match core's
  `SymConstraint` tag shape.

## Grades

| Level | Grade | Rationale |
|---|---|---|
| L1 | C+ | Careful error typing and fallback logic, but a 16k-line file, a hand-rolled JSON encoder that emits invalid JSON silently, stdout protocol collision in crate-bridge, doubled test runs, and silent offline skips |
| L2 | C | Per-crate CLAUDE.md is rich but drifts: runtime flavor, single-file claims, line refs, preflight, console-capture parity, timeout table |
| L4 | C+ | Sound plugin boundaries (runtime crate, llm plugin, adapter registry); weaker on duplicate encoders/classifiers, inverted timeout budget, no panic boundary, O(N²) registry |
| L5 | C | Simple Rust functions with int `match`/arithmetic/string-escape conditions get partial concolic coverage because runtime constraints degrade to unknown or string equality; printing functions are unexecutable in crate-bridge |
| AGENT | C | Gate cache and affected-selector holes, tracker duplicates, stale memory note, new work contradicting a filed decision |
