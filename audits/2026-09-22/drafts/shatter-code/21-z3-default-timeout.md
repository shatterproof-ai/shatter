# Z3 has no default per-query timeout (solver can outlive timeout_explore), and `scan --solver-timeout` is silently discarded

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | solver,z3,timeout,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-36cd, str-w0d.3 |
| source findings | core-12 |

<!-- body -->
## Problem

Without `--solver-timeout`, Z3 queries run unbounded; with solver offload the blocking call cannot be cancelled by deadline checks. `scan` accepts the flag and throws it away.

## Current code facts / evidence

- `shatter-cli/src/args.rs:658-660`, `:1074-1075`: help says 'Default: no limit'.
- `shatter-core/src/solver.rs:1214-1217`: `set_timeout_msec` only when Some.
- Exception: `shatter-cli/src/helpers.rs:893` defaults 10 s under --mcdc.
- `shatter-cli/src/main.rs:593` destructures Scan with `solver_timeout: _`, so scan always passes None.

## Acceptance criteria

- Default per-query timeout (e.g. 2 s, configurable) applied in all modes; help updated.
- Z3 Unknown/timeout is recorded as a frontier stall (visible in artifact stats).
- `scan --solver-timeout N` is honoured; CLI test asserts it reaches the solver config.

## Suggested approach

Set default in one place (solver config construction); wire scan.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: core-12 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-36cd, str-w0d.3
