---
slug: engine-path-identity-budget-config
kind: new
title: "Engines disagree on what a 'path' is, --max-iterations means different budgets per entry point, and three hand-built orchestrator::ExploreConfig literals diverge"
priority: P2
type: task
labels: [audit-2026-09-22, architecture, parity, explorer, orchestrator]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Engines disagree on what a 'path' is, --max-iterations means different budgets per entry point, and three hand-built orchestrator::ExploreConfig literals diverge

## Problem

1. **Path identity.** The random explorer and the concolic orchestrator hash "paths" differently. On a loop function with two behaviours, concolic reports 16 paths and the random explorer 6. The random explorer's own shrink-witness selection uses the concolic hash rather than its path hash, so one engine even uses two identities.
2. **Budget semantics.** Concolic `max_iterations` is a unique-path cap, while `max_executions` is set to 1x `--max-iterations` in `explore` and 5x in `scan` and `observe`. The help text for all of them says "Maximum number of iterations per function".
3. **Config construction.** `explore`, `scan` and `observe` each build `orchestrator::ExploreConfig` by hand, with different seeds, budgets, refine settings, execute plans, mocks and solver timeouts.

Users therefore cannot compare results across commands or engines, and benchmarks such as concolic-vs-default-benchmark need to control for all of this by hand.

## Evidence (re-verified at audit HEAD 56c86168)

- Random path hash: `shatter-core/src/explorer.rs:566-571` (`path_hash`: scope-aware + loop buckets, with the `legacy_path_hash` line/error/return fallback).
- Concolic path hash: `shatter-core/src/orchestrator.rs:864-871` (`hash_branch_path`: raw `(branch_id, taken)` sequence, `DefaultHasher`). Unique-path cap check: `orchestrator.rs:1621-1627`.
- Random explorer shrink selection calls `crate::orchestrator::hash_branch_path` (`explorer.rs:1704`, `:1788`, `:1833`), not `path_hash`. This is also the explorer<->orchestrator cycle tracked in str-qwua7.29.
- Fixture used by the audit (`areas/core-engine.md:10-25`):

  ```go
  func Loopy(n int) int {
      s := 0
      for i := 0; i < n && i < 50; i++ { s += i }
      if s > 100 { return 1 }
      return 0
  }
  ```

  Result: 16 paths under `--concolic` (40 iterations) vs 6 under random (100 iterations), for 2 behaviours. The concolic report lists 16 rows.
- `max_executions`:
  - `shatter-cli/src/commands/explore.rs:5145` = `max_iterations` (1x);
  - `shatter-core/src/scan_orchestrator.rs:3144-3150` `concolic_scan_max_executions` = 5x, or 1x with custom generators;
  - `shatter-cli/src/commands/observe.rs:109` = 5x.
- Help: `shatter-cli/src/args.rs:505` and `:992` read "Maximum number of iterations per function [default: 100]", and `:1379` reads "(default: 50)".
- The three literals:
  - `explore.rs:5138-5169`
  - `scan_orchestrator.rs:3080-3102`
  - `observe.rs:107-130`

  How they differ:

  | Field | explore | scan | observe |
  |---|---|---|---|
  | `seed` | None | `explore_config.seed` | None |
  | `refine_budget` | set | None | None |
  | `default_execute_plan` | None | threaded | None |
  | `mcdc` | flag | false | false |
  | `fuzz` | resolved | default | default |
  | `mocks` / `mock_params` | from config | from config | empty |
  | `solver_timeout_ms` | from flag | from flag | None |
  | `plateau_threshold` | 20 or 60 | 20 | 20 |

## Acceptance criteria

- [ ] Path identity lives in one leaf module (for example `shatter-core/src/path_identity.rs`) used by the random explorer, the concolic orchestrator and both shrinkers. This also removes the explorer<->orchestrator `hash_branch_path` cycle (coordinate with str-qwua7.29).
- [ ] One constructor (for example `orchestrator::ExploreConfig::from_explorer(&explorer::ExploreConfig, Overrides)`) builds the orchestrator config for explore, scan and observe. The three struct literals are gone (verify with `grep -n "orchestrator::ExploreConfig {"` over non-test code), and unit tests assert that shared fields round-trip.
- [ ] `--max-iterations` has one documented meaning across explore, scan, observe and run, and the help text in `args.rs` says what it bounds (unique paths vs executions).
- [ ] Engine-parity test: Loopy and `01-arithmetic` give equal path counts under random and concolic. Proof at close: the test fails on current main (paste the failure) and passes after the change. It can be added as a row in the engine_parity suite (engine-parity-e2e) if that lands first.

## Suggested approach

Coordinate with str-qwua7.6.2 (shared ExploreConfig base; it does not name the scan or observe literals) and str-qwua7.20.1. Either do the work there and append this issue's scope as a note, or do it here as a child with those linked. Land the path-identity module first; the budget and config unification follows.

## Out of scope

- Splitting `explore_with_oracle` into phases (str-qwua7.6).
- Other random-vs-concolic drifts: setup, mocks, refine and capture have their own issues in shatter-engine-correctness.

## Metadata

- Priority: P2
- Type: task
- Labels: audit-2026-09-22, architecture, parity, explorer, orchestrator
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: str-qwua7.6.2, str-qwua7.20.1, str-qwua7.29, str-inct, engine-parity-e2e, concolic-vs-default-benchmark
- Source findings: core-07, core-14 (draft shatter-code/17)
