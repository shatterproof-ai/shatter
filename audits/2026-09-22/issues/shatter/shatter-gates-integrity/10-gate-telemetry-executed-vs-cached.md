---
slug: gate-telemetry-executed-vs-cached
kind: new
title: "Gate telemetry can't tell executed from cached runs (budgets computed from no-ops): add a per-leaf event stream, aggregate it, and recompute budgets from new executed-only measurements"
priority: P2
type: task
labels: [quality-gates, performance, resource-governance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [task-list-json-poisons-checksums, ci-executed-leaf-guard]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Gate telemetry can't tell executed from cached runs

## Problem

Between 18% and 28% of recorded gate runs fail, and a cold full check takes more than 40 minutes on the shared machine under contention. Meanwhile the recorded median `check` wall time mostly reflects cached no-ops. The budgets in `docs/perf/gate-budgets.md` are computed from rows that mix runs whose leaves executed with runs whose leaves were cached, so both slowness and hollowness are invisible in the telemetry.

The telemetry cannot be fixed by adding a column. `scripts/gate-wrapper.sh` writes **one row per outer invocation** (`check`, `affected`, ...). Nested wrapped tasks (`check` → `conformance`, `core:test-ignored`, ...) hit the `SHATTER_GATE_LOCK_HELD` pass-through (`gate-wrapper.sh:21-23`, `exec "$@"`) and record nothing. So there is no per-leaf identity or duration anywhere in the data, and historical rows cannot be reclassified after the fact.

## Evidence (re-verified 2026-09-23 on main `70465921`)

- `~/.cache/shatter/gate-times.csv` (since 2026-08-10; columns `timestamp,worktree,label,wall_seconds,exit_code,loadavg_1min,slot,wait_seconds`, per `scripts/gate-wrapper.sh:9-10` and the `csv_write` function): non-zero exits are affected 44/156, check-fast 36/151 and check 26/143. There is no executed-vs-cached information.
- `gate-wrapper.sh` runs the gate without capturing its output (`nice ... "$@" &`, then `wait`), so it has nothing to parse today.
- In the audit run, workspace clippy alone took 14m48s and 22m01s at load 100-190, and stage 2 alone took 1558 s (finding gates-11; not re-timed).

## Acceptance criteria

- [ ] **Event schema, documented in the `gate-wrapper.sh` header:** the outer (lock-holding) invocation gets an `invocation_id`. Each leaf reached under it produces one row in a separate file, for example `~/.cache/shatter/gate-leaves.csv`, with `invocation_id,timestamp,label_outer,leaf,outcome,wall_seconds`, where `outcome` ∈ `executed|cached|missing|failed`. The existing per-invocation CSV keeps its columns and gains `invocation_id`, so rows join.
- [ ] **Source of leaf data:** the outer invocation tees the gate's output to a temp log and classifies leaves with the shared parser from `ci-executed-leaf-guard` (not a new regex). Per-leaf `wall_seconds` comes from nested wrapped leaves recording their start/end into the invocation's temp dir before the pass-through `exec` (or an equivalent mechanism); leaves without a wrapper may record an empty duration. The chosen mechanism is stated in the header.
- [ ] A unit test wired into `meta` runs the wrapper around a stub command that prints one executed leaf, one cached leaf and one failing leaf, and asserts the rows written to both CSVs (including matching `invocation_id`).
- [ ] **Aggregation:** a committed script (for example `scripts/gate-budgets.py`) computes per-gate and per-leaf medians and p90s from `executed` rows only, and reports how many `cached`/`missing` rows it excluded.
- [ ] **New measurements, not history:** `docs/perf/gate-budgets.md` is regenerated only from rows recorded after this change and after the str-qwua7.3 fix, with at least 10 executed `check` invocations. The doc names the script, the date range and the row counts, and states that pre-change rows are excluded because they cannot distinguish execution from caching.

## Suggested approach

Tee inside `run_governed`, pass an `invocation_id` and a temp dir to nested calls through the environment the wrapper already sets for `SHATTER_GATE_LOCK_HELD`, and have the pass-through path append its leaf's timing there before `exec`. Parse the teed log after the child exits. Keep CSV writes under the existing `csv.lock`.

## Out of scope

- Enabling sccache for gate runs (`sccache-for-gate-runs`, split from this issue).
- Host-wide admission control across projects, including making gate-wrapper's slot and bento's `run-heavy` share one budget (bento-dyp7).
- Receipts proving gates ran (str-qwua7.2, str-35vtk.24/.25). The CI guard (`ci-executed-leaf-guard`).
- The pressure-aware wait policy and gate event log (str-35vtk.19, str-35vtk.18); this issue's CSV is the timing log, not that event store.

## Metadata

- Priority: P2. Type: task. Size: M.
- Labels: quality-gates, performance, resource-governance, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: task-list-json-poisons-checksums (the str-qwua7.3 fix), ci-executed-leaf-guard (shared parser).
- Related: str-35vtk.10, str-35vtk.24, str-35vtk.19, str-0f6ze, bento-dyp7, `sccache-for-gate-runs`.
- Source findings: gates-11. Draft shatter-code/10.
