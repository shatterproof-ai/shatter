# Gate telemetry can't tell executed from cached runs; sccache is installed but unused; machine-wide slot covers only shatter gates

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | task |
| priority | P2 |
| labels | quality-gates,performance,sccache,resource-governance,audit |
| parent | audit epic (draft 00) |
| blocked by | draft 01 |
| related | str-35vtk.10, str-35vtk.24, str-35vtk.2, bento-dyp7 |
| source findings | gates-11 |

<!-- body -->
## Problem

16-28% of recorded gate runs fail and cold checks take 40+ minutes on the shared machine, while the median recorded `check` (149 s) mostly reflects cached no-ops. Budgets in docs/perf/gate-budgets.md are computed from rows that mix executed and cached leaves.

## Current code facts / evidence

- `~/.cache/shatter/gate-times.csv` (since 2026-08-10): affected 44/156, check-fast 36/151, check 26/141 non-zero exits; no executed/cached column.
- `/usr/bin/sccache` exists; `RUSTC_WRAPPER` unset in env; `.cargo/config` has no rustc-wrapper. str-35vtk.2 (closed) documented a host-level sccache setup and rejected a shared CARGO_TARGET_DIR.
- `scripts/gate-wrapper.sh` semaphore governs shatter gates only; other agents' cargo/jest runs are unbounded.

## Acceptance criteria

- gate-wrapper records per-leaf executed vs cached in the CSV.
- gate-budgets.md is recomputed from executed rows only (script committed).
- Either sccache is enabled for gate runs (documented in AGENTS.md with measured effect) or the decision not to is recorded here with reason.

## Suggested approach

Parse Task's `is up to date` lines in gate-wrapper; add the column; re-derive budgets.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: Host-wide admission control across projects (bento-dyp7).
- Size: S-M

## References

- Audit findings: gates-11 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-35vtk.10, str-35vtk.24, str-35vtk.2, bento-dyp7
