# Correct stale and false facts in the four crate CLAUDE.md files (core, ts, go, rust)

- Priority: P2
- Type: task
- Labels: docs,agents,parity
- Tracker action: new issue for the fact corrections. Append a note to str-qwua7.25 that it can drop the ".js references" item once this lands. Generation from the parity matrix stays in str-qwua7.24, and dangling-ID resolution stays in str-qwua7.34.
- Related: str-qwua7.24, str-qwua7.25, str-qwua7.34, str-qwua7.59, str-u394l.4
- Source findings: audit 2026-09-22 core-21, frontend-ts-15, frontend-go-09, frontend-rust-08 (all confirmed)

<!-- body -->
## Problem
Subagents are primed with the crate CLAUDE.md files, which are auto-injected. Each has false claims that send agents to the wrong code or the wrong contract.

## shatter-core/CLAUDE.md
- `:7` says `explorer.rs — Concolic exploration loop`. explorer.rs is the random/hybrid engine; `orchestrator.rs` is the concolic engine (listed at `:17` as "Multi-round exploration orchestration").
- `:11` lists `export.rs`, which is dead code (str-qwua7.59). `:3` mentions "export logic".
- Missing entries: `solver.rs`, `strategy.rs`, `shrink.rs`, `pipeline_orchestrator.rs`, `planner_consumer.rs`.

## shatter-ts/CLAUDE.md
- `:16-17` cite "Lines ~278-352" and "~860-951". The real locations are about 874 and 1862. Use symbol names.
- `:44` says "TS is the only frontend that produces ite … Go and Rust do not". `protocol/parity-matrix.yaml:1100` says Go does too.
- `:52` and `:208` cite divergence IDs `loop-body-states-typescript-only` and `error-code-preflight-failed-typescript-only`, which have 0 matches in the matrix.
- `:379-380` cite `src/browser-globals-recognizer.js` and `src/handlers.js`. The files are `.ts`.
- `:375` says "str-jeen.40 will refine bucketing", but str-jeen.40 is closed.
- Key Files lists 2 of about 30 modules.

## shatter-go/CLAUDE.md (54,640 bytes)
- `:206` (and `shatter-go/protocol/constants.go:16-19`, `types.go:584-590`) say Go does not emit `preflight_failed`, but `protocol/handler.go:394` sets it.
- `:52-62` name `instrument/flow.go` and `flowwalk.go` as the `ite` mechanism. Both are unreachable per `deadcode ./...`.
- `:273` says `planner.ResolveMockSpecs` emits MockSpecs. It is unreachable.
- It cites `str-8v66` and `str-ruw0`; `bd show` finds neither.
- `:318` mentions `SHATTER_HARNESS_CACHE`, which no Go source reads.
- `:138` says "TS and Rust currently declare `outcome` only".
- The outcome status list omits `skipped_by_policy` and `preflight_failed`.

## shatter-rust/CLAUDE.md
- `:109` (and parity-matrix `async_runtime` notes, around line 887) say `Runtime::new().block_on`. The code uses `Builder::new_current_thread` (`executor.rs:2547,2845,4942,6801`) on purpose.
- `:79` and `:93` cite a "single-file constraint", but `analyzer.rs:834-952` resolves same-crate cross-file types.
- `:234` cites `handler.rs:552/620/803`. The real lines are 693, 767 and 842/970. Use symbol names.
- The parity matrix marks Rust `console_output: captured`, but crate-bridge mode does not capture it. Mark it partial.
- The parity-matrix axum note omits `Multipart`.
- The Rust-frontend timeout fallback is 120 s (`executor.rs:1017`), but `PARITY.md:96` says 30 s.

## Acceptance criteria
- Every item above is corrected, or removed in favour of a pointer to the parity matrix.
- Line-number references are replaced with symbol names.
- A check (in `task docs` or drift-patrol; natural home str-u394l.4) fails when a crate CLAUDE.md cites a `src/` path that does not exist or a `str-*` ID that `bd show` cannot resolve. Divergence-ID resolution is str-qwua7.34.

## Scope
In: fact corrections and the path/ID check. Out: generated capability tables (str-qwua7.24), size slimming (str-qwua7.25).
