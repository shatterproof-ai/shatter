---
slug: mock-variation-reopen-note
kind: reopen-note
title: "Note on closed str-3ky9.4: concolic mock variation was undone by str-lebv/str-r59s"
priority: P2
type: bug
labels: [concolic, mocking, regression, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [concolic-mock-variation-regression]
existing_id: str-3ky9.4
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on closed str-3ky9.4: concolic mock variation was undone by str-lebv/str-r59s

**Target:** str-3ky9.4 (closed). Add a comment. Do not reopen. Substitute the filed id of concolic-mock-variation-regression for the slug when posting, after that issue is filed.

## Comment text

> **Audit 2026-09-22 (finding core-04): regressed after close.**
>
> The per-worklist-entry mock variation added here (e0313728) is no longer active in the concolic engine:
>
> - 0293c35c (str-lebv, MetaStrategy wiring, 2026-03-14) changed `solve_and_generate` to take `_mock_params: &[MockParam]`, which is unused (`shatter-core/src/orchestrator.rs:2147`).
> - 9f2d2a3d (str-r59s, 2026-03-23) added `_initial_mocks` ("Retained for future use; currently unused"), which is generated and discarded (`orchestrator.rs:2640-2647`) and still consumes RNG draws.
> - Strategy worklist entries carry `mock_values: vec![]` (`orchestrator.rs:2223`, `:2259`) and fall back to the fixed `config.mocks`.
> - `input_gen::mutate_mock_values` now has only test callers (`input_gen.rs:8081`, `:8533`).
>
> The random explorer still varies mocks on every iteration, so the two engines diverge. Restoring the variation (or explicitly documenting and warning about fixed mocks) is tracked in **<id of concolic-mock-variation-regression>**. Line numbers verified at `56c86168`.
