---
slug: z3-default-query-timeout
kind: new
title: "Z3 has no default per-query timeout (solver can outlive timeout_explore), and scan --solver-timeout is silently discarded"
priority: P2
type: bug
labels: [solver, z3, timeout, cli, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Z3 has no default per-query timeout (solver can outlive timeout_explore), and scan --solver-timeout is silently discarded

## Problem

Without `--solver-timeout`, Z3 queries have no time limit. With solver offload, the Z3 call runs on a blocking thread, which the explore deadline checks cannot cancel. One hard query can therefore run past `timeout_explore` and the per-function budget. The only exception is `--mcdc`, which defaults to 10 s.

Separately, `shatter scan` accepts `--solver-timeout` and throws it away, so scan always runs with no solver limit.

## Evidence

Line numbers were re-checked against `56c86168`:

- `shatter-cli/src/args.rs:658`, `:1073` and `:1420`: the `--solver-timeout` help reads "Z3 solver timeout in seconds per query. Default: no limit."
- `shatter-core/src/solver.rs:1214-1217`, `:1254-1257` and `:1343-1346`: all three solver entry points call `cfg.set_timeout_msec(ms)` only `if let Some(ms) = solver_timeout_ms`.
- `shatter-cli/src/helpers.rs:893-897`: the only default, `Some(10)` when `mcdc && solver_timeout.is_none()`.
- `shatter-cli/src/main.rs:593`: the `Scan` arm destructures `solver_timeout: _`, so the flag never reaches the scan solver config.
- The 2026-09-04 audit's claim "Timeouts: cfg.set_timeout_msec on each query" holds only when a value is set.

## Acceptance criteria

- [ ] A default per-query Z3 timeout (e.g. 2 s, configurable through the flag and `.shatter/config.yaml`) applies in all modes and commands. It is set in one place, at solver-config construction. The `--mcdc` 10 s default is either kept as a documented override or folded into the same mechanism.
- [ ] The `--solver-timeout` help text and any docs state the new default.
- [ ] A Z3 `Unknown`/timeout result is recorded as a frontier stall and is visible in artifact stats (a counter in the explore artifact), not silently treated as UNSAT.
- [ ] `shatter scan --solver-timeout N` is honored. A CLI test asserts that the value reaches the solver config used by scan. At close, show the test failing on current `main` and passing after the fix.
- [ ] A test with a deliberately hard query (for example nonlinear integer arithmetic) completes within the default timeout plus a small margin and records the stall.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass. Run `task gauntlet` if help output snapshots change.

## Suggested approach

Resolve the effective timeout once, where the budgets are resolved (`resolve_mcdc_budgets` in `helpers.rs` or its successor), and pass `Some(default)` down instead of `None`. Wire `solver_timeout` through the `Scan` arm in `main.rs` the same way as for explore. Add the stall counter where `SolveResult::Unknown` is handled in the orchestrator.

## Out of scope

- Complexity-aware timeout allocation (str-w0d.3, deferred).
- Making blocking Z3 calls cancellable.

## Priority

P2

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-36cd (closed; added the optional flag with no default), str-w0d.3.

## References

Audit 2026-09-22 finding core-12 (verified, P2; the verifier found the discarded `scan --solver-timeout`). Source draft: `drafts/shatter-code/21-z3-default-timeout.md`.
