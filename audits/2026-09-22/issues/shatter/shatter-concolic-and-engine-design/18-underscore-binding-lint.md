---
slug: underscore-binding-lint
kind: new
title: "Lint _-prefixed parameters and let bindings in shatter-core/shatter-cli non-test code (they hid the concolic mock-variation regression)"
priority: P3
type: task
labels: [audit-2026-09-22, quality-gates, lint, shatter-core]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Lint _-prefixed parameters and let bindings in shatter-core/shatter-cli non-test code (they hid the concolic mock-variation regression)

## Problem

A leading `_` silences rustc's unused warning. In the orchestrator it hid a real regression: mock parameters were accepted and then dropped, so dynamic mock variation stopped working under `--concolic` (concolic-mock-variation-regression):

- `shatter-core/src/orchestrator.rs:2147`: `_mock_params: &[MockParam]`
- `orchestrator.rs:2643`: `let _initial_mocks = ...`

Nothing flags a new `_`-prefixed binding, so the next dropped input will be just as silent. This was part of the combined draft engine-parity-e2e and is split out as a separate deliverable.

## Evidence

A rough grep at audit HEAD finds about 92 `_`-prefixed `let` and parameter bindings across `shatter-core/src` and `shatter-cli/src`, including test modules (same-runtime cross-check count). The non-test share has not been measured.

## Acceptance criteria

- [ ] A script under `scripts/`, wired into `task check-static`, flags `_`-prefixed function parameters and `let` bindings in `shatter-core/src` and `shatter-cli/src`, excluding:
  - `#[cfg(test)]` modules and `tests/` directories;
  - bare `_` and `let _ = ...` (explicit discard);
  - bindings with a `// allow-underscore: <reason>` or `TODO(str-...)` comment on the same or previous line.

  The exclusion rules are written in the script's header.
- [ ] Before wiring, the close note records the number of existing non-test hits. Each is fixed (binding used or removed) or annotated with a reason or issue ID. No blanket allowlist file.
- [ ] Proof at close: the script run on a branch with a planted `fn f(_unused: u32)` in `shatter-core/src` (fails, naming the file and line); the script passing on the final branch; the `task check-static` output showing the step ran uncached.

## Out of scope

- Fixing the mock-variation regression (concolic-mock-variation-regression).
- Other crates.

## Metadata

- Priority: P3
- Type: task
- Labels: audit-2026-09-22, quality-gates, lint, shatter-core
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: concolic-mock-variation-regression, engine-parity-e2e
- Source findings: core-22 (split from draft shatter-agent/23)
