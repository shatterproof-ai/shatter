# VS Code Extension Design

Automatic exploratory testing feedback directly in the editor. Surface branch coverage, triggering inputs, behavioral specs, and exploration progress where the developer is already looking.

## Design Principle

**Surface concolic intelligence where the developer is already looking.** Don't make them switch to a terminal or dashboard — bring branch discovery, missing coverage, and generated inputs directly into the code they're reading.

## Existing Tools Survey

| Tool | License | Fit |
|---|---|---|
| VS Code Test Explorer UI | MIT | Good adapter base for test results, but only pass/fail |
| Coverage Gutters | MIT | Pattern to emulate for gutter decorations (line-level only) |
| Wallaby.js | Commercial | Gold standard UX reference (real-time inline feedback) |
| VS Code Extension API | MIT | The actual platform to build on |
| SARIF Viewer | MIT | Could export results as SARIF, but oriented toward static analysis |

**Decision:** Build a native VS Code extension. Borrow UX patterns from Coverage Gutters (gutter decorations) and Wallaby (inline feedback).

## Feature Tiers

### Tier 1 — MVP

1. **Branch Coverage Gutters** — After explore/scan, render gutter icons per line (green/yellow/red). Hover shows branch condition, discovered inputs, path count. Data source: coverage index JSON.

2. **CodeLens: "Explore this function"** — Above every exported function: `Explore with Shatter | 3/5 branches covered`. Click runs `shatter explore --function <name> <file>` in background. Second CodeLens: `Generate tests (3 cases)`.

3. **Diagnostics Integration (Problems Panel)** — Scan results as VS Code diagnostics: Warning (uncovered branches), Info (stale function), Error (crash on generated input).

4. **Generated Test Preview** — Command opens virtual document with generated tests. "Accept" writes to disk.

### Tier 2 — High Value

5. **Inline Branch Annotations** — Inside if/switch/ternary, show ghost text with triggering inputs: `if (email.indexOf('@') === -1)  <- true: "foo", false: "a@b.com"`. Most distinctive feature.

6. **Exploration Progress Panel (TreeView)** — Sidebar showing per-function coverage status with live updates during exploration.

7. **Behavioral Spec Hover** — Hover over function name shows behavioral spec table (inputs -> outputs).

8. **Stale Function Indicator** — On save, compare fingerprints. Modified functions get "stale" badge. "Re-explore stale functions" command.

### Tier 3 — Differentiating

9. **Symbolic Constraint Visualization** — For uncovered branches, show WHY (Z3 unsolvable, opaque dependency, etc.).

10. **Diff-Aware Exploration** — On git diff, identify changed functions and offer to explore before commit.

11. **Custom Generator Scaffolding** — When opaque types detected, Quick Fix scaffolds generator file.

## New Output Types Required

### Output 1: Branch Coverage Index

Write to `.shatter/coverage/<file-hash>.json` after exploration. Maps lines to branch coverage status and triggering inputs.

```json
{
  "version": 1,
  "file_path": "src/validators.ts",
  "file_hash": "sha256:abc...",
  "generated_at": "2026-03-05T12:00:00Z",
  "functions": [
    {
      "name": "validateEmail",
      "start_line": 10,
      "end_line": 42,
      "branches": [
        {
          "branch_id": 1,
          "line": 15,
          "branch_type": "If",
          "condition_text": "email.indexOf('@') === -1",
          "arms": [
            {
              "taken": true,
              "covered": true,
              "discovery_method": "Z3",
              "triggering_inputs": [["\"foo\""]],
              "outcome_summary": "returns false"
            },
            {
              "taken": false,
              "covered": true,
              "discovery_method": "UserProvided",
              "triggering_inputs": [["\"a@b.com\""]],
              "outcome_summary": "continues"
            }
          ]
        }
      ],
      "coverage_pct": 80.0,
      "iterations": 47,
      "termination_reason": "WorklistExhausted"
    }
  ]
}
```

**Production:** Post-process `ExploreResult` in a new `build_coverage_index()` function. Walk `FunctionAnalysis.branches` + `raw_results` to find representative inputs per branch arm. Merge with `discoveries` for attribution.

**Noise impact:** None — reshaping already-computed data into a file in gitignored `.shatter/`.

### Output 2: Exploration Event Stream

Add `ExploreEvent` enum and optional `tokio::sync::mpsc::UnboundedSender` to `orchestrator.explore()`:

```rust
pub enum ExploreEvent {
    IterationComplete { iteration: u32, unique_paths: usize, total_executions: usize },
    NewPathDiscovered { path_index: usize, inputs: Vec<Value>, outcome_summary: String, discovery_method: DiscoveryMethod, branch_decisions: Vec<(u32, bool)> },
    ConstraintSolved { branch_id: u32, branch_line: u32, solver_time_ms: f64 },
    ConstraintUnsolvable { branch_id: u32, hint: String },
    Plateau { consecutive_no_new: u32, threshold: u32 },
    FunctionComplete { function_name: String, result_summary: CoverageMetrics, termination_reason: TerminationReason },
}
```

CLI emits NDJSON to stderr when `--events` flag is passed. Extension reads stderr line by line.

**Noise impact:** Zero when unused — `Option<Sender>` is `None` by default, branch predicted away.

### Output 3: Fingerprint Manifest

Write `.shatter/fingerprints.json` mapping `file -> function -> {fingerprint, explored_at, coverage}`. Extension reads this to detect stale functions without running CLI.

**Noise impact:** Negligible — one small file write at end of each run.

## Real-Time Exploration on Edit

### The Constraint

Exploration is irreducibly expensive (2-10s per function). True keystroke-level reactivity is not possible for exploration. But staleness detection, static analysis, and cached result display can be instant.

| Operation | Latency | Real-time? |
|---|---|---|
| Staleness detection (fingerprint diff) | <10ms | Yes — on every save |
| Static analysis (branch count, params) | ~200ms | Yes — on save with debounce |
| Show cached results for unchanged functions | 0ms | Yes — file watch |
| Exploration of changed function | 2-10s | No — show progress, update when done |

**Decision:** Trigger on save, not on keystroke. Code in mid-edit is often syntactically invalid.

### Phased Approach (Recommended)

**Phase 1 — File-watch + CLI spawn (MVP):**
- Extension watches file saves, computes fingerprint diffs
- Spawns `shatter explore --function <fn> <file> --events --coverage-index`
- Reads event stream for live progress, coverage index for final results
- No daemon, no new process architecture

**Phase 2 — Warm frontend pool:**
- Extension keeps one frontend subprocess alive per language
- Sends protocol messages directly to warm frontend (eliminates ~350ms cold start)
- Analysis cache in `.shatter/cache/` (already exists)

**Phase 3 — Background daemon:**
- `shatter daemon` process with JSON-RPC interface
- Handles prioritization (function under cursor first), cancellation, incremental analysis
- Extension becomes thin UI layer

### Daemon Architecture (Phase 3)

```
VS Code Extension          shatter daemon
+-----------------+        +------------------------+
|                 | stdin/  |                        |
| - watches .json |<-stdout>| - Analysis Cache       |
| - sends edits   |        | - Frontend Pool        |
| - renders UI    |        |   (TS/Go/Rust alive)   |
|                 |        | - Coverage Index writer |
|                 |        | - Priority queue        |
+-----------------+        +------------------------+
```

Amortized costs vs cold CLI spawn:
- Frontend subprocess startup: ~300ms -> 0ms
- Protocol handshake: ~50ms -> 0ms
- File analysis: ~200ms -> ~50ms (incremental)
- Exploration: 2-10s -> 2-10s (irreducible)

Daemon supports cancellation: when user edits a function mid-exploration, cancel in-flight work and restart with new code.

## Changes to shatter-core/shatter-cli

| Change | Files | Complexity | Noise Risk |
|---|---|---|---|
| `build_coverage_index()` | `explorer.rs` or new `coverage_index.rs` | Low | None |
| Write `.shatter/coverage/*.json` | `main.rs` | Low | None |
| `ExploreEvent` enum + channel | `orchestrator.rs` | Medium | None when `None` |
| `--events` CLI flag | `main.rs` | Low | None — opt-in |
| `--coverage-index` flag | `main.rs` | Low | None |
| Fingerprint manifest | `main.rs` | Low | None |
| Cancellation token | `orchestrator.rs` | Medium | Minimal |

All changes are additive and opt-in. No impact on existing output, tests, or protocol.

## Tech Stack

- **Extension:** TypeScript (VS Code standard)
- **Bundler:** esbuild
- **Testing:** `@vscode/test-electron`
- **Package:** `vsce` for marketplace
