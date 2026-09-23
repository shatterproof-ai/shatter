---
slug: concolic-mock-variation-regression
kind: new
title: "Concolic dynamic mock variation regressed (str-3ky9.4 undone by str-lebv/str-r59s); hidden behind _-prefixed params"
priority: P2
type: bug
labels: [concolic, mocking, regression, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Concolic dynamic mock variation regressed (str-3ky9.4 undone by str-lebv/str-r59s); hidden behind _-prefixed params

## Problem

str-3ky9.4 ("Orchestrator dynamic mock variation", closed 2026-03-10 in e0313728) added per-worklist-entry mock variation to the concolic orchestrator. Later MetaStrategy wiring made the mock parameters unused, and `_` prefixes silenced the compiler warning that would have flagged it. The concolic engine now explores with fixed mocks (`config.mocks`), while the random explorer still regenerates mocks on every iteration. Any branch that depends on a mocked dependency's return value can be unreachable under `--concolic`.

The generate-and-discard block also still consumes RNG draws, which shifts the rest of the seeded run for no benefit.

## Evidence

Line numbers were re-checked against `56c86168`:

- `shatter-core/src/orchestrator.rs:2640-2647`: `let _initial_mocks = if !config.mock_params.is_empty() { input_gen::generate_mock_values(...) } else { vec![] };` with the comment "Retained for future use; currently unused". `git log -S` attributes it to 9f2d2a3d (str-r59s, 2026-03-23).
- `shatter-core/src/orchestrator.rs:2147`: `solve_and_generate(.., _mock_params: &[MockParam], ..)` came from 0293c35c (str-lebv, 2026-03-14).
- Worklist entries built from strategies carry `mock_values: vec![]` (`orchestrator.rs:2223`, `:2259`, and also `:2894`, `:2915`), so they fall back to `config.mocks`.
- `input_gen::mutate_mock_values` (`input_gen.rs:4241`) has only test callers (`input_gen.rs:8081`, `:8533`).
- The random explorer regenerates mocks per iteration (`explorer.rs:1537`, `:2468`).
- There is no parity test for mock variation between the engines. The existing tests that look like one do not exercise the concolic engine: `concolic_mock_status_branches_discovered` (`shatter-core/tests/e2e_concolic.rs:1713`) and `concolic_mock_result_branches_discovered` (`:1809` area) use the fixture `standalone/ts/17-mock-branches.ts` from the external examples repo but call `shatter_core::explorer::explore_function`, the random explorer. Their doc comment says so: "Uses the random explorer (not orchestrator) because it regenerates mock values per iteration". The concolic name hides the regression.

## Acceptance criteria

Restoring per-entry mock variation in the concolic engine is the required outcome. Documenting fixed mocks and warning instead is not an acceptable resolution for this issue; if the maintainer decides against restoring it, close this issue as won't-fix and file that change separately.

- [ ] Worklist entries produced by the concolic MetaStrategy loop carry varied mock values (through `input_gen::mutate_mock_values`, or the random explorer's `generate_mock_values` path) whenever `config.mock_params` is non-empty. `mock_values: vec![]` is no longer used for strategy-produced entries at `orchestrator.rs:2223`, `:2259`, `:2894` and `:2915` when mock params exist.
- [ ] The dead `_initial_mocks` block (`orchestrator.rs:2640-2647`) is removed, and `_mock_params` in `solve_and_generate` (`:2147`) is used (the leading underscore is dropped) or removed from the signature.
- [ ] A new concolic E2E test in `shatter-core/tests/e2e_concolic.rs` runs `classifyStatus` from `standalone/ts/17-mock-branches.ts` through `orchestrator::explore` (not `explorer::explore_function`) and asserts that all four returns ("empty", "short", "medium", "long") are reached. At close, quote the test output from current `main` (fails) and after the fix (passes). The test is `#[ignore]`d like its neighbours, so run it through `task e2e-ts` (or `cargo test --test e2e_concolic -- --include-ignored <name>`) and quote the line showing it ran.
- [ ] The two existing random-explorer tests are renamed so their names do not say "concolic" (for example `random_mock_status_branches_discovered`).
- [ ] A seeded determinism test: two concolic runs with the same seed and non-empty `mock_params` produce the same sequence of mock values.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Restore the variation in the MetaStrategy loop, drawing from the orchestrator's seeded RNG. When a strategy produces a worklist entry, attach `mutate_mock_values(...)` output, as the random explorer does with `generate_mock_values`. Reuse the random explorer's generation helper rather than adding a third one.

## Out of scope

- Redesigning mock configuration or the mock-substitution frontends.
- The shared Execute builder (concolic-refine-execute-builder), though it should carry the per-entry mocks once they exist.

## Priority

P2: the verifier lowered core-04 from P1 to P2. This is a coverage-quality regression of a closed feature, not incorrect output.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-3ky9.4 (closed but not fixed; gets mock-variation-reopen-note), str-lebv, str-r59s, str-3ky9.6.

## References

Audit 2026-09-22 finding core-04 (verified, corrected P1 to P2). Report §11.3 lists str-3ky9.4 as closed-but-unfixed. Source draft: `drafts/shatter-code/14-concolic-mock-variation-regression.md`.
