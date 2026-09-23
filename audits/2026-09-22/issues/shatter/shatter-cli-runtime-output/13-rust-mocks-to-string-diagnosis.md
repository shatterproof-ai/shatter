---
slug: rust-mocks-to-string-diagnosis
kind: new
title: "Diagnose why the Rust explore report lists `Mocks: to_string` for safe_divide"
priority: P3
type: task
labels: [rust-frontend, mocks, report, diagnosis, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Diagnose why the Rust explore report lists `Mocks: to_string` for safe_divide

## Problem

The explore report for `rust/04_errors.rs:safe_divide` lists `Mocks: to_string`. `safe_divide` presumably calls `"division by zero".to_string()`, a standard-library conversion that should not be mocked. Either the mock-recording source reports a symbol that was not replaced (a reporting bug), or `to_string` really is being replaced (an execution-correctness bug that could change observed outcomes). The audit did not determine which. Split out of per-language-outcome-rendering during the cross-check; the placement of the `Mocks:` line stays there.

## Evidence

- `audits/2026-09-22/cli-ux-transcripts/rust-explore3.out` line 10: `- *Mocks: to_string*` after the `safe_divide` table.
- `shatter-cli/src/render.rs:139-142` renders `opts.mocks_used`, which the explore command passes as `mock_symbols` (`shatter-cli/src/commands/explore.rs:3620-3628`).
- Not re-verified by the audit verifier (cli-ux-11 note).

## Acceptance criteria

This is a diagnosis issue. It closes with a written finding, not necessarily a fix.

- [ ] Trace where `mock_symbols` for a Rust target comes from (frontend response field and the shatter-rust code that fills it) and state it in the close reason with file:line references.
- [ ] Determine, with a test or a recorded run, whether `to_string` is actually replaced during execution of `safe_divide` (for example: does the `Err` payload still equal `"division by zero"` under exploration, and does the instrumented source substitute the call).
- [ ] Close reason states one of: (a) reporting-only bug, (b) real mock of a std method, (c) intended behavior, with the evidence. For (a) or (b), file a follow-up bug with a failing test attached (or fix it here if it is under ~20 lines, with the test red on main and green after). For (c), document the behavior in `shatter-rust/CLAUDE.md`.

## Related

In this bucket: per-language-outcome-rendering (renders the Mocks line).

## Priority / Type

P3, task (diagnosis).
