---
slug: sccache-for-gate-runs
kind: new
title: "sccache is installed but unused by gate runs: measure a cold `task check-unit` with and without it, then enable it or record why not"
priority: P3
type: task
labels: [quality-gates, performance, sccache, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [task-list-json-poisons-checksums]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# sccache is installed but unused by gate runs

## Problem

Cold Rust builds dominate gate wall time on the shared machine (a cold full check took more than 40 minutes under contention in the audit). Every new linked worktree starts with an empty target dir. `sccache` is installed on the host but nothing in the repo or the gate environment uses it, and no measurement shows whether it would help. str-35vtk.2 (closed) documented a host-level sccache setup and rejected a shared `CARGO_TARGET_DIR`, but did not wire sccache into gates or measure it.

## Evidence (re-verified 2026-09-23)

- `/usr/bin/sccache` is installed. `RUSTC_WRAPPER` is unset in the agent environment, and `.cargo/config.toml` has no `rustc-wrapper`.
- In the audit run, workspace clippy alone took 14m48s and 22m01s at load 100-190 (finding gates-11; not re-timed).

## Acceptance criteria

- [ ] A reproducible measurement, with commands recorded in the close reason: in two fresh linked worktrees of the same commit (after the str-qwua7.3 fix, so the leaves actually execute), run `task --force check-unit` once without sccache and once with `RUSTC_WRAPPER=sccache` and a pre-warmed sccache cache (warmed by one prior run in a third worktree). Record wall time, load average at start, and `sccache --show-stats` hit rate.
- [ ] Decision recorded: either sccache is enabled for gate runs (in `gate-wrapper.sh`, or `.cargo/config.toml` behind a guard that falls back cleanly when sccache is absent, with a `meta` test for the fallback), with the before/after numbers in AGENTS.md; or the close reason records the numbers and why it was not enabled.
- [ ] If enabled, a cold `task check-unit` on a machine without sccache still passes (the fallback test above), so CI and other hosts are unaffected.

## Out of scope

- A shared `CARGO_TARGET_DIR` (rejected in str-35vtk.2).
- Gate telemetry (`gate-telemetry-executed-vs-cached`), which should supply the executed-only timings once it lands.

## Metadata

- Priority: P3. Type: task. Size: S.
- Labels: quality-gates, performance, sccache, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: task-list-json-poisons-checksums (the str-qwua7.3 fix; before it, `check-unit` leaves may not execute).
- Related: str-35vtk.2, `gate-telemetry-executed-vs-cached`.
- Source findings: gates-11 (sccache part; split from `gate-telemetry-executed-vs-cached` in the 2026-09-23 Codex revision). Draft shatter-code/10.
