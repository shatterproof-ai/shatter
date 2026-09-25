---
slug: explore-budget-semantics
kind: new
title: "--max-iterations means a different budget per command and engine (concolic max_executions 1x in explore, 5x in scan/observe); give it one meaning and an explicit execution budget"
priority: P1
type: task
labels: [audit-2026-09-22, cli, explorer, orchestrator, parity]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# --max-iterations means a different budget per command and engine (concolic max_executions 1x in explore, 5x in scan/observe); give it one meaning and an explicit execution budget

## Problem

The help for `--max-iterations` on explore and scan says "Maximum number of iterations per function [default: 100]" (`shatter-cli/src/args.rs:505`, `:992`). observe says "(default: 50)" (`:1379`). What the flag actually bounds depends on the command and the engine:

- Under `--concolic`, `max_iterations` is a **unique-path** cap (`shatter-core/src/orchestrator.rs:1621-1627`), and a separate `max_executions` caps total executions (`:1628-1634`).
- `max_executions` is derived differently per command, and no flag sets it:
  - `explore`: `max_executions = max_iterations` (1×), `shatter-cli/src/commands/explore.rs:5145`;
  - `scan`: `concolic_scan_max_executions` = 5×, or 1× with custom generators (`shatter-core/src/scan_orchestrator.rs:3144-3150`);
  - `observe`: 5× (`shatter-cli/src/commands/observe.rs:109`).

The same `--max-iterations 100` therefore allows 100 executions in `explore --concolic` and 500 in `scan --concolic`. A default-vs-concolic comparison cannot set equal budgets (concolic-vs-default-benchmark, which is blocked on this issue).

This was part of the combined draft engine-path-identity-budget-config. It is split out because it is independently deliverable and on the D3 critical path.

## Acceptance criteria

- [ ] `--max-iterations` has one documented meaning across explore, scan, observe and run, for both engines. The help text on all four says exactly what it bounds (unique paths or executions) and gives the same default, or states why a command's default differs.
- [ ] One flag (for example `--max-executions`) sets the total-execution budget directly on explore, scan, observe and run, for both engines. Defined once in a shared options struct flattened into each command (the str-qwua7.20.1 direction).
- [ ] The derived default for `max_executions`, when the flag is absent, is computed by one function used by all commands. The 1× and 5× literals at `explore.rs:5145`, `scan_orchestrator.rs:3144-3150` and `observe.rs:109` are gone.
- [ ] Each run's effective budget (unique-path cap and execution cap) is written to the explore/scan artifact or summary, so a harness can read it back.
- [ ] Tests: for each of explore, scan, observe and run, and for both engines, a test asserts the effective budget built from the same flags is identical. A CLI test asserts that `--max-executions 30` stops a concolic run at 30 executions with `stop_reason: max_executions` (depends on explore-stop-reason-accounting for the explore path). Proof: the per-command test fails on current main (paste it) and passes after.
- [ ] SPEC.md's `--max-iterations` entry and the new flag are documented. The `task gauntlet` flag-permutation step exercises the new flag. Forced (uncached) `task e2e` and `task gauntlet` output at close.

## Out of scope

- Path identity (engine-path-identity-budget-config).
- The rest of the `orchestrator::ExploreConfig` unification (str-qwua7.6.2; qwua7-6-2-scan-observe-config-literals).

## Metadata

- Priority: P1 (blocks the D3 benchmark)
- Type: task
- Labels: audit-2026-09-22, cli, explorer, orchestrator, parity
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Blocks: concolic-vs-default-benchmark
- Related: str-qwua7.20.1 (shared ExploreOptions), str-qwua7.6.2, engine-path-identity-budget-config, explore-stop-reason-accounting
- Source findings: core-14 (split from draft shatter-code/17)
- Decision refs: D3
