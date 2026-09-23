---
slug: concolic-refine-execute-builder
kind: new
title: "Concolic refine phase sends Execute with prepare_id: None and execution_profile: None, dropping TS execution adapters"
priority: P2
type: bug
labels: [concolic, orchestrator, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Concolic refine phase sends Execute with prepare_id: None and execution_profile: None, dropping TS execution adapters

## Problem

`refine_boundaries_async` in the concolic orchestrator builds its own `Command::Execute` requests with `prepare_id: None` and `execution_profile: None`. Every other Execute site in `orchestrator.rs` passes `prepare_id.clone()` and `config.execution_profile.clone()`. The refine phase therefore runs without the target's execution profile, which drops TS execution adapters, and it cannot reuse the prepared build.

This issue is only the request-field fix. The related work was split out after the cross-check:

- counting refine-phase executions as discovered paths: concolic-refine-path-accounting;
- one shared Execute-request builder for all sites: execute-request-builder;
- the `capture` flag at every site: open str-qwua7.5, which owns it.

## Evidence

Line numbers were re-checked on the audit branch, whose code is identical to `56c86168`:

- `shatter-core/src/orchestrator.rs:2321` `async fn refine_boundaries_async(...)` takes `setup_context` and `execute_plan` but no `prepare_id` or execution profile. Its Execute at `:2375-2384` sends `setup_context: setup_context.clone(), capture: false, prepare_id: None, execution_profile: None`. It is called from `:3527`.
- The other seven orchestrator Execute sites pass both fields: `:1695-1696`, `:2702-2703`, `:2715-2716`, `:3104-3105`, `:3644-3645`, `:3688-3689`, `:3742-3743`.
- Not verified: the audit guessed that a missing `prepare_id` forces a rebuild per Execute on Go and Rust. The close note should state what actually happens.

## Acceptance criteria

- [ ] `refine_boundaries_async` receives the run's `prepare_id` and `config.execution_profile`, and its Execute requests carry them. `capture` stays as it is; str-qwua7.5 owns that field.
- [ ] A unit test with a recording frontend double (a `Frontend` stand-in that records every `Command` it receives) runs a concolic exploration that enters the refine phase with a non-`None` `prepare_id` and `execution_profile`. It asserts that every Execute request sent during the refine phase carries both values. At close, quote the test failing on current `main` and passing after the fix.
- [ ] A TS execution-adapter target explored with `--concolic` reaches the refine phase without adapter errors. Name the target and quote the relevant log lines in the close note.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Thread `prepare_id: Option<&str>` and `execution_profile` through the `refine_boundaries_async` signature from its one caller at `:3527`, which already has both in scope.

## Out of scope

- Refine-phase path accounting (concolic-refine-path-accounting).
- The shared Execute builder (execute-request-builder).
- The `capture` flag (str-qwua7.5).

## Priority

P2

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-qwua7.5 (open; owns `capture` at every site), concolic-refine-path-accounting, execute-request-builder.

## References

Audit 2026-09-22 finding core-06 (verified, P2). Source draft: `drafts/shatter-code/16-concolic-refine-phase-execute-builder.md`. Split after the Codex cross-check, which found that the earlier draft bundled three deliverables and conflicted with str-qwua7.5's ownership.
