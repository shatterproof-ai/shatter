---
slug: concolic-setup-teardown
kind: new
title: "Configured setup files are silently ignored under --concolic (every production caller passes setup_context=None), and the random explorer shrinks after teardown"
priority: P1
type: bug
labels: [concolic, setup, orchestrator, parity, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Configured setup files are silently ignored under --concolic (every production caller passes setup_context=None), and the random explorer shrinks after teardown

## Problem

Ownership of setup and teardown is inconsistent between the two engines.

Setup files are configured in `.shatter/config.yaml` (`defaults.setup`, `defaults.setup_level`, `defaults.setup_timeout`, and per-function `functions.<target>.setup`; see `docs/resource-parameters.md:60-75` and `shatter-core/src/config.rs:889-903`). There is no `--setup` CLI flag. The CLI only has `--setup-timeout` and `--fail-on-setup-error` (`shatter-cli/src/args.rs:699`, `:703`).

1. **Concolic ignores configured setup.** `orchestrator::explore` and `orchestrator::explore_with_oracle` accept a `setup_context`, but every production caller passes `None`. So `shatter explore <file> --concolic`, with a setup file configured, runs without setup and prints no warning. str-0s76.6 ("setup in concolic") was closed with "All callers updated". Its E2E injects a context directly into `orchestrator::explore`, so it passes even though the real pipeline never builds one. This issue replaces that closed issue; see the reopen-note setup-parity-reopen-note.
2. **The random explorer shrinks after per-function teardown.** Per-function teardown runs before the witness-shrinking phase, and the shrink Execute calls pass `setup_context: None`. Witnesses found under setup state are replayed without that state. The shrinker then either spends its budget on rejections or accepts a witness that depends on state that no longer exists. The orchestrator's copy of the shrink uses `setup_context.clone()`.
3. **Execution-level setup is not applied to shrink attempts in either engine.** With `setup_level: execution`, the random explorer's main loop runs setup and teardown around each execution (`shatter-core/src/explorer.rs:1629-1638` for the teardown). The shrink Execute calls are not bracketed that way, so each shrink attempt runs without the per-execution state.

## Reproduction

In a fresh directory with a TS target `f.ts` whose branch depends on state created by a setup file (for example, the setup writes a file and the target branches on `existsSync` of it):

```yaml
# .shatter/config.yaml
defaults:
  setup: "./setup/make-fixture.ts"
  setup_level: function
```

`shatter explore f.ts --clean` reaches the setup-dependent branch. `shatter explore f.ts --concolic --clean` does not, and prints no warning that setup was skipped.

## Evidence

Line numbers were re-checked on the audit branch, whose code is identical to `56c86168`:

- `shatter-core/src/orchestrator.rs:2453` `explore(...)` and `:2490` `explore_with_oracle(...)` take `setup_context: Option<SetupContextStack>` as the 7th argument.
- The production callers all pass `None` in that position:
  - `shatter-core/src/pipeline_orchestrator.rs:536-548` (`explore_with_oracle`, 7th arg `None` at :543)
  - `shatter-core/src/scan_orchestrator.rs:3103-3115` (`explore`, `None` at :3110)
  - `shatter-cli/src/commands/observe.rs:179-190` (`explore`, `None` at :186)
- `grep -c teardown shatter-core/src/orchestrator.rs` returns 0. `send_setup` is called only from `explorer.rs` and `observe.rs`.
- `shatter-cli/src/commands/explore.rs:5051` copies the resolved config value `resolved.setup` into `setup_file` (and `:5052` `setup_level`) of the random explorer's `ExploreConfig` only. No warning for `--concolic` was found.
- `shatter-core/tests/e2e_concolic.rs:1555` `orchestrator_explore_with_setup_context` calls `orchestrator::explore` directly with a hand-built context.
- `shatter-core/src/explorer.rs:1076-1077` derive `per_function_setup` and `per_execution_setup` from `config.setup_level` (`SetupLevel` has `Session`, `File`, `Function`, `Execution`; `shatter-core/src/protocol.rs:31-36`).
- `explorer.rs:1676-1683` runs per-function `send_teardown` before the `-- Witness shrinking phase --` at `:1692+`. The shrink Execute calls pass `setup_context: None` at `:1778`, `:1823` and `:1875`. The orchestrator's shrink passes `setup_context: setup_context.clone()` (`orchestrator.rs:3642`, `:3686`, `:3740`). Both copies came from 4d8001bc9 (str-28ea.6).
- Scan and observe set `setup_file: None` everywhere, so neither engine supports setup there. Whether that is intended is undocumented.

## Required lifecycle

The fix must implement this contract in both engines. Session- and file-level setup are owned outside the per-function explore call and are unchanged here.

| `setup_level` | Main exploration loop | Shrink phase | Teardown |
|---|---|---|---|
| `function` | one Setup before the first execution; every Execute carries its context | runs before function teardown; every shrink Execute carries the same live context | once, after the shrink phase |
| `execution` | Setup before and Teardown after each Execute | each shrink attempt is bracketed by its own Setup and Teardown, as in the main loop | after each attempt |

## Acceptance criteria

- [ ] One pipeline-level helper implements the lifecycle table above, and both engines receive their setup context from it. `run_pipeline`, `explore_with_scan_mode` and `observe` all use it (or reject setup explicitly, see below).
- [ ] A CLI-level E2E, not one that calls `orchestrator::explore` directly, uses a `.shatter/config.yaml` like the Reproduction above and shows that the setup side effect is visible to the target under `shatter explore <file> --concolic`, for `setup_level: function` and for `setup_level: execution`. TS is the minimum. Add Go and Rust if their frontends declare setup support in `protocol/parity-matrix.yaml`. At close, quote the test output from current `main` (fails) and after the fix (passes). If the test is `#[ignore]`d, run it with `-- --include-ignored` or its `task e2e-*` target, and quote the lines showing it ran.
- [ ] Shrink tests in both engines, with setup and shrinking enabled, one per level:
  - `function`: every shrink Execute request carries the live context, no Teardown request is sent before the last shrink Execute, and the shrunk witness still reproduces its path.
  - `execution`: every shrink Execute is preceded by a Setup and followed by a Teardown, and the shrunk witness still reproduces its path.
  - Assert on the sequence of requests sent (for example through a recording frontend double), so the test cannot pass under a level that never runs setup.
- [ ] If scan and observe intentionally do not support setup files, a configured setup under those commands produces an explicit error or warning, and a CLI test covers that. Otherwise they are wired through the same helper.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Move setup and teardown out of `explorer.rs` into the pipeline layer, so both engines receive a live context and teardown happens after the shrink phase in both. For execution-level setup, pass the engines a callback (or a small trait) that brackets one Execute with Setup/Teardown, and use it in both the main loop and the shrink loop. This also prepares str-qwua7.6.1's shared `select_witnesses`/shrink extraction, which currently leaves shrink behavior unchanged. The Execute-request builder (execute-request-builder) should take `setup_context` from this helper.

## Out of scope

- Adding setup support to frontends that lack it.
- Session- and file-level setup semantics.
- The shared Execute builder itself (execute-request-builder).

## Priority

P1: configured setup is silently dropped in the default-recommended engine, and a closed issue claims otherwise.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-0s76.6 (closed but not fixed; gets setup-parity-reopen-note), str-0s76.12, str-qwua7.6.1, execute-request-builder.

## References

Audit 2026-09-22 findings core-03 (P1) and core-09 (P2, folded in). Report §11.3 lists str-0s76.6 as closed-but-unfixed, and §15.1 files this as new rather than reopening. Source draft: `drafts/shatter-code/13-concolic-setup-teardown-ownership.md`. Revised after the Codex cross-check: the draft previously cited a nonexistent `--setup` flag.
