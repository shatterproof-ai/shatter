# --setup is silently ignored in concolic mode (all production callers pass setup_context=None), and the random explorer shrinks after teardown

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | concolic,setup,orchestrator,parity,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-0s76.6, str-0s76.12, str-qwua7.6.1 |
| source findings | core-03, core-09 |

<!-- body -->
## Problem

Setup/teardown is owned inconsistently. `orchestrator::explore` accepts `setup_context` but every production caller passes `None`, so `explore --concolic --setup f` runs without setup and gives no warning. str-0s76.6 was closed on an E2E that injects the context directly. Separately, the random explorer tears down before shrinking and shrinks with `setup_context: None`, so witnesses found under setup are replayed without it.

## Current code facts / evidence

- `shatter-core/src/orchestrator.rs:2460/2497` take `setup_context` as 7th arg; `pipeline_orchestrator.rs:542`, `scan_orchestrator.rs:3109`, `shatter-cli/src/commands/observe.rs:186` pass `None`. orchestrator.rs contains no teardown call; `send_setup` is called only from explorer.rs and observe.rs.
- `shatter-cli/src/commands/explore.rs:5051` resolves --setup into the random explorer config only.
- `shatter-core/tests/e2e_concolic.rs:1555` `orchestrator_explore_with_setup_context` calls `orchestrator::explore` directly.
- `shatter-core/src/explorer.rs:1676-1683` teardown runs before shrink (:1692+); shrink Execute calls at :1778, :1823, :1875 use `setup_context: None` (orchestrator shrink uses `setup_context.clone()` at :3642).
- scan and observe set `setup_file: None` everywhere (no setup support in either engine there).

## Acceptance criteria

- One pipeline-level helper sends Setup, passes the context to either engine, and sends Teardown after shrink; used by run_pipeline, explore_with_scan_mode and observe.
- CLI/pipeline-level E2E: a setup side effect is visible to the target under `explore --concolic --setup` (TS at minimum).
- Random-explorer shrink runs before teardown with the live context; a test with setup + shrink enabled passes.
- If scan/observe intentionally do not support --setup, passing it errors or warns explicitly.

## Suggested approach

Lift setup/teardown out of explorer.rs into the pipeline layer; this also prepares str-qwua7.6.1's shared shrink extraction.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: core-03, core-09 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-0s76.6, str-0s76.12, str-qwua7.6.1
