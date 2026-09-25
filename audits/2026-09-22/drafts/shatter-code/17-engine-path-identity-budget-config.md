# Engines disagree on what a 'path' is, --max-iterations means different budgets per entry point, and three hand-built orchestrator::ExploreConfig literals diverge

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | task |
| priority | P2 |
| labels | architecture,parity,explorer,orchestrator,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.6.2, str-qwua7.20.1, str-qwua7.29, str-inct |
| source findings | core-07, core-14 |

<!-- body -->
## Problem

Random and concolic explorers hash paths differently (concolic reports 16 'paths' for a loop with 2 behaviours vs 6 in random), the concolic `max_iterations` is a unique-path cap while `max_executions` is 1x or 5x depending on entry point, and explore/scan/observe each build `orchestrator::ExploreConfig` by hand with different seeds, budgets, mocks and plans.

## Current code facts / evidence

- Random path hash: `shatter-core/src/explorer.rs:566-571` (scope-aware + loop-bucket, line/error/return fallback). Concolic: `orchestrator.rs:864-871`, `:1621-1627` raw (branch_id,taken) sequence.
- Loopy(n) fixture: 16 paths concolic (40 iters) vs 6 random (100 iters).
- max_executions: 1x at `shatter-cli/src/commands/explore.rs:5145`; 5x in scan (`scan_orchestrator.rs:3144-3150`, 1x with custom generators); 5x in `observe.rs:109`. Help (args.rs:505/992): 'Maximum number of iterations per function'.
- Config literals: explore.rs:5138-5169, scan_orchestrator.rs:3080-3101, observe.rs:107-130. Differences: seed (None / threaded / None), refine_budget (set/None/None), default_execute_plan (None/threaded/None), observe mocks/mock_params empty and solver_timeout None.

## Acceptance criteria

- Path identity lives in one leaf module used by both engines and the shrinker (also resolves the explorer↔orchestrator `hash_branch_path` cycle, str-qwua7.29).
- One constructor (e.g. `From<&explorer::ExploreConfig>` plus explicit overrides) builds orchestrator config for explore/scan/observe; unit tests assert shared fields round-trip.
- --max-iterations has one documented meaning across explore/scan/observe/run; help text says it.
- Engine-parity test: same fixtures (Loopy, 01-arithmetic) give equal path counts under random and concolic.

## Suggested approach

Fold into str-qwua7.6.2 / str-qwua7.20.1 work (append note there), or do as a child with those as related.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: L

## References

- Audit findings: core-07, core-14 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.6.2, str-qwua7.20.1, str-qwua7.29, str-inct
