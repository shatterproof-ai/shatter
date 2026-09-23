---
slug: concolic-setup-teardown
kind: new
title: "--setup is silently ignored under --concolic (every production caller passes setup_context=None), and the random explorer shrinks after teardown"
priority: P1
type: bug
labels: [concolic, setup, orchestrator, parity, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# --setup is silently ignored under --concolic (every production caller passes setup_context=None), and the random explorer shrinks after teardown

## Problem

Ownership of setup and teardown is inconsistent between the two engines.

1. **Concolic ignores `--setup`.** `orchestrator::explore` and `orchestrator::explore_with_oracle` accept a `setup_context`, but every production caller passes `None`. So `shatter explore --concolic --setup f` runs without setup and prints no warning. str-0s76.6 ("setup in concolic") was closed with "All callers updated". Its E2E injects a context directly into `orchestrator::explore`, so it passes even though the real pipeline never builds one. This issue replaces that closed issue; see the reopen-note setup-parity-reopen-note.
2. **The random explorer shrinks after teardown.** Per-function teardown runs before the witness-shrinking phase, and the shrink Execute calls pass `setup_context: None`. Witnesses found under setup state are replayed without that state. The shrinker then either spends its budget on rejections or accepts a witness that depends on state that no longer exists. The orchestrator's copy of the shrink correctly uses `setup_context.clone()`.

## Evidence

Line numbers were re-checked against `56c86168`:

- `shatter-core/src/orchestrator.rs:2453` `explore(...)` and `:2490` `explore_with_oracle(...)` take `setup_context: Option<SetupContextStack>` as the 7th argument.
- The production callers all pass `None` in that position:
  - `shatter-core/src/pipeline_orchestrator.rs:536-548` (`explore_with_oracle`, 7th arg `None` at :543)
  - `shatter-core/src/scan_orchestrator.rs:3103-3115` (`explore`, `None` at :3110)
  - `shatter-cli/src/commands/observe.rs:179-190` (`explore`, `None` at :186)
- `grep -c teardown shatter-core/src/orchestrator.rs` returns 0. `send_setup` is called only from `explorer.rs` and `observe.rs`.
- `shatter-cli/src/commands/explore.rs:5051` resolves `--setup` into `setup_file` for the random explorer config only. No CLI warning for `--concolic` was found.
- `shatter-core/tests/e2e_concolic.rs:1555` `orchestrator_explore_with_setup_context` calls `orchestrator::explore` directly with a hand-built context.
- `shatter-core/src/explorer.rs:1676-1683` runs per-function `send_teardown` before the `-- Witness shrinking phase --` at `:1692+`. The shrink Execute calls pass `setup_context: None` at `:1778`, `:1823` and `:1875`. The orchestrator's shrink passes `setup_context: setup_context.clone()` (`orchestrator.rs:3642`, `:3686`, `:3740`). Both copies came from 4d8001bc9 (str-28ea.6).
- Scan and observe set `setup_file: None` everywhere, so neither engine supports setup there. Whether that is intended is undocumented.

## Acceptance criteria

- [ ] One pipeline-level helper sends Setup, passes the resulting context to whichever engine runs, and sends Teardown after shrinking. `run_pipeline`, `explore_with_scan_mode` and `observe` all use it.
- [ ] A CLI-level or pipeline-level E2E, not one that calls `orchestrator::explore` directly, shows that a setup side effect is visible to the target under `shatter explore --concolic --setup <file>`. TS is the minimum. Add Go and Rust if their frontends declare setup support in `protocol/parity-matrix.yaml`. At close, show the test failing on current `main` and passing after the fix.
- [ ] Random-explorer shrinking runs before teardown with the live setup context. A test with setup and shrinking both enabled asserts that the shrink Execute requests carry the context and that the shrunk witness still reproduces its path.
- [ ] If scan and observe intentionally do not support `--setup`, passing it errors or warns explicitly, and a CLI test covers that. Otherwise they are wired through the same helper.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Move setup and teardown out of `explorer.rs` into the pipeline layer, so both engines receive a live context and teardown happens after the shrink phase in both. This also prepares str-qwua7.6.1's shared `select_witnesses`/shrink extraction, which currently leaves shrink behavior unchanged. The Execute-request builder in concolic-refine-execute-builder should take `setup_context` from this helper.

## Out of scope

- Adding setup support to frontends that lack it.
- The shared Execute builder itself (concolic-refine-execute-builder).

## Priority

P1: a user flag is silently dropped in the default-recommended engine, and a closed issue claims otherwise.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-0s76.6 (closed but not fixed; gets setup-parity-reopen-note), str-0s76.12, str-qwua7.6.1, concolic-refine-execute-builder.

## References

Audit 2026-09-22 findings core-03 (P1) and core-09 (P2, folded in). Report §11.3 lists str-0s76.6 as closed-but-unfixed, and §15.1 files this as new rather than reopening. Source draft: `drafts/shatter-code/13-concolic-setup-teardown-ownership.md`.
