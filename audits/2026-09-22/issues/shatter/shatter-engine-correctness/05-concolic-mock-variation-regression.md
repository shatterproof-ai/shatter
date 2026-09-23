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
- There is no parity test for mock variation between the engines.

## Acceptance criteria

- [ ] Concolic worklist entries carry varied mock values (through `mutate_mock_values` or an equivalent), or concolic documents that mocks are fixed and warns at runtime when `mock_params` is non-empty. Record which option was taken in the close note. If the second is chosen, update `protocol/parity-matrix.yaml` / the relevant CLAUDE.md engine notes.
- [ ] The dead `_initial_mocks` block is removed, and `_mock_params` is either used or removed from the signature.
- [ ] A concolic E2E fixture whose branch depends on a mocked dependency's return value reaches both sides under `--concolic` (TS at minimum, in `shatter-core/tests/e2e_concolic.rs`). At close, show the test failing on current `main` and passing after the fix.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Restore the variation in the MetaStrategy loop. When a strategy produces a worklist entry, attach `mutate_mock_values(...)` output, as the random explorer does with `generate_mock_values`. Reuse the random explorer's generation helper rather than adding a third one.

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
