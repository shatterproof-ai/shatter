---
slug: gate-telemetry-executed-vs-cached
kind: new
title: "Gate telemetry can't tell executed from cached runs (budgets computed from no-ops); sccache installed but unused; machine-wide slot covers only shatter gates"
priority: P2
type: task
labels: [quality-gates, performance, sccache, resource-governance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [task-list-json-poisons-checksums]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Gate telemetry can't tell executed from cached runs; sccache is installed but unused; the machine-wide slot covers only shatter gates

## Problem

Between 18% and 28% of recorded gate runs fail, and a cold full check takes more than 40 minutes on the shared machine under contention. Meanwhile the recorded median `check` wall time mostly reflects cached no-ops. The budgets in `docs/perf/gate-budgets.md` are computed from rows that mix leaves that executed with leaves that were cached, so both slowness and hollowness are invisible in the telemetry. Governance was designed per project, so other agents' cargo and jest runs are not bounded by the shatter semaphore.

## Evidence (re-verified 2026-09-23)

- `~/.cache/shatter/gate-times.csv` (since 2026-08-10; columns `timestamp,worktree,label,wall_seconds,exit_code,loadavg_1min,slot,wait_seconds`, per `scripts/gate-wrapper.sh:9-10,55`): non-zero exits are affected 44/156, check-fast 36/151 and check 26/143. There is no executed-vs-cached column.
- In the audit run, workspace clippy alone took 14m48s and 22m01s at load 100-190, and stage 2 alone took 1558 s (finding gates-11; not re-timed).
- `/usr/bin/sccache` is installed, `RUSTC_WRAPPER` is unset in the environment, and `.cargo/config.toml` has no `rustc-wrapper`. str-35vtk.2 (closed) documented a host-level sccache setup and explicitly rejected a shared `CARGO_TARGET_DIR`.
- The `scripts/gate-wrapper.sh` semaphore (`SHATTER_HEAVY_SLOTS` flock slots, header `:1-15`) governs shatter gates only. Other projects' cargo and jest runs, and shatter work outside gate-wrapper (hooks, see `fast-hermetic-precommit`), are unbounded.

## Acceptance criteria

- [ ] `gate-wrapper.sh` records, for each leaf, whether it executed or was cached (parsed from Task's `Task "X" is up to date` lines, or from a flag passed by the task) as a new CSV column. The format change is documented in the script header.
- [ ] A unit test wired into `meta` feeds the wrapper's parser a sample log with one cached and one executed leaf and asserts the column values.
- [ ] `docs/perf/gate-budgets.md` is recomputed from executed rows only, by a committed script. The doc names the script and the date range used.
- [ ] Either sccache is enabled for gate runs, with a measured before/after cold `task check-unit` time recorded in AGENTS.md, or the reason for not enabling it is recorded in this issue's close reason.
- [ ] Either gate-wrapper's slot is honoured by bento's `run-heavy` (or vice versa), so cross-project heavy runs share one budget, or this is explicitly deferred to bento-dyp7 in the close reason.

## Suggested approach

Have gate-wrapper tee the wrapped command's output (it currently runs it without capturing output), parse `is up to date` lines from it, and add the column. Write `scripts/gate-budgets.py` to derive budgets from executed rows. Try `RUSTC_WRAPPER=sccache` in gate-wrapper, or in `.cargo/config.toml` behind an env guard, and measure. Do this after str-qwua7.3's fix lands. Before then, nearly every stage-2/3 row is a cached no-op and the recomputed budgets would be meaningless.

## Out of scope

- Host-wide admission control across projects (bento-dyp7), beyond the interoperability decision above.
- Receipts proving gates ran (str-qwua7.2, str-35vtk.24/.25).
- The CI guard (`ci-executed-leaf-guard`).

## Metadata

- Priority: P2. Type: task. Size: S-M.
- Labels: quality-gates, performance, sccache, resource-governance, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: task-list-json-poisons-checksums (the str-qwua7.3 fix).
- Related: str-35vtk.10, str-35vtk.24, str-35vtk.2, str-0f6ze, bento-dyp7.
- Source findings: gates-11. Draft shatter-code/10.
