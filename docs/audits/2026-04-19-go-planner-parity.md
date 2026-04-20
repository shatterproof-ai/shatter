# Go Planner Parity Audit

**Date:** 2026-04-19
**Issue:** str-hy9b.H1
**Scope:** Determine whether the Go planner artifact (`invocation_plan`, str-hy9b.E1) is wired into Shatter's two explorer paths — the random explorer (`shatter-core/src/explorer.rs`) and the concolic orchestrator (`shatter-core/src/orchestrator.rs`) — and whether a `--planner` CLI flag exists.

## Summary

The Go planner is **not wired** into either explorer path. It is declared as an optional, Go-only capability in the parity matrix, but there is no Rust-side consumer, no protocol wire type, no CLI flag, and no call site in the explorer or orchestrator. Every integration point required to make the capability usable end-to-end is missing.

## Integration points

| # | Component | Location | Status |
|---|---|---|---|
| 1 | Parity matrix declaration | `protocol/parity-matrix.yaml:578-587` | **Declared only** — `invocation_plan` listed as `go: supported`, `typescript/rust: not_supported`, with a forward-reference to str-hy9b.E1 |
| 2 | Go frontend capability note | `shatter-go/CLAUDE.md:77` | **Declared only** — lists `invocation_plan` as a Go-only capability |
| 3 | E2E gated stub | `shatter-core/tests/e2e_concolic.rs:1643-1651` | **Placeholder** — `#[ignore]` test referencing "str-hy9b.E1 (planner skeleton)" with an asserted `receiver_kind: "new_service"` stub; gated on D3-D6 + E1 |
| 4 | `--planner` CLI flag | `shatter-cli/src/args.rs` (Explore command) / `shatter-cli/src/main.rs` | **Missing** — no flag exists; ~50 other Explore flags are present (e.g. `--concolic`, `--genetic`, `--strategy-weights`) but none select a planner |
| 5 | Random explorer integration | `shatter-core/src/explorer.rs` | **Missing** — zero `planner` references in the file; input generation hard-wired via `ValueSource`, `generate_inputs_with_custom`, mock params; `ExploreConfig` has no planner field |
| 6 | Concolic orchestrator integration | `shatter-core/src/orchestrator.rs` | **Missing** — zero `planner` references; input generation uses `solver::solve_for_new_path` (Z3) + drilling; `ExploreConfig` has no planner field |
| 7 | Rust protocol type | `shatter-core/src/protocol.rs` | **Missing** — no `InvocationPlan` request/response struct; Rust core has no wire-format means to request a plan |
| 8 | Go protocol type | `shatter-go/protocol/types.go` | **Missing** — no `InvocationPlan` struct on the Go side either; the capability is declared in the parity matrix but the schema has not been defined |

## Gaps and follow-up issues

Each gap has been filed as a `bd` issue (type: feature, priority: 2, labels: `go,parity,redesign`). Dependency order is encoded in `bd` — str-zbyp blocks the other three.

### Gap 1 — No `--planner` CLI flag → **str-x20u**

`shatter-cli/src/args.rs` (Explore command) and `shatter-cli/src/main.rs` expose no planner selection. The user cannot request planner-driven invocation even if the Go frontend were to support it.

### Gap 2 — Random explorer does not consume planner output → **str-4z2b**

`shatter-core/src/explorer.rs` has no planner hook. Input generation is closed over `ValueSource` and mock parameters; there is no path by which a planner artifact from the Go frontend could influence invocation selection.

### Gap 3 — Concolic orchestrator does not consume planner output → **str-3iwg**

`shatter-core/src/orchestrator.rs` has no planner hook. Per the project parallel-paths rule (top-level `CLAUDE.md`), capabilities added to `explorer.rs` are routinely missing from `orchestrator.rs`; both wirings must land together. This issue tracks the orchestrator half.

### Gap 4 — No `InvocationPlan` protocol type (Rust + Go) → **str-zbyp** *(blocker)*

Neither `shatter-core/src/protocol.rs` nor `shatter-go/protocol/types.go` defines an `InvocationPlan` request/response schema. Without the wire format, Rust core cannot request a plan from the Go frontend even though Go declares the capability. This issue blocks str-x20u, str-4z2b, and str-3iwg.

## Recommended order of work

1. **str-zbyp** — define the wire schema (request name, response fields, serialization), regenerate protocol bindings, add round-trip tests on both sides.
2. **str-x20u** — add `--planner <strategy>` to the Explore command (and wherever else the capability is user-selectable); plumb the choice into `ExploreConfig`.
3. **str-4z2b** and **str-3iwg** *(in parallel, landed together)* — thread the planner through both the random explorer and the concolic orchestrator so a single capability flip exercises both code paths. Grep for the parallel call sites before closing either.
4. Re-enable `shatter-core/tests/e2e_concolic.rs` planner test (lines 1643-1651) once the wire format and at least one explorer path are live.

## Verification

This audit is docs-only. `npx task test-standard` was run and passed to confirm no incidental changes.
