---
slug: go-explore-warmup-gate
kind: new
title: "Explore lacks the cold-build warmup gate that scan has; many-target Go explores on a cold cache can time out en masse"
priority: P3
type: bug
labels: [go-frontend, explore, parity, performance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Explore lacks the cold-build warmup gate that scan has; many-target Go explores on a cold cache can time out en masse

## Problem

str-tbk9e fixed a cold-cache thundering herd for **scan** by adding `BuildWarmupGate`: the first harness build of a run goes alone, then the other workers fan out once the build cache is warm. **Explore** never got the gate. A multi-target Go explore at default parallelism starts all workers on a cold build cache, each triggers a full build at once, and under load they all hit the 30 s request timeout.

This is a parity gap between the scan and explore paths (see the "parallel parity" rule in the root CLAUDE.md). The user-facing failure was seen only on a heavily loaded host, so it is P3 until it is reproduced on a quiet one. The fix is still worth making for parity, and its acceptance is a deterministic ordering test rather than a timing test on a quiet host, which could not distinguish fixed from unfixed behaviour.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `grep -rn "BuildWarmupGate\|warmup_gate" shatter-core/src shatter-cli/src` finds hits only in `shatter-core/src/scan_orchestrator.rs` (struct at `:2138`, `WarmupLeaderGuard` at `:2181`, created at `:3870`, entered at `:3300`); none on the explore path (`shatter-cli/src/commands/explore.rs`, `shatter-core/src/explorer.rs`, `orchestrator.rs`). Existing unit tests of the gate: `scan_orchestrator.rs:6634` `warmup_gate_serializes_first_task_then_fans_out`, `:6682`, `:6699`.
- str-tbk9e (closed, P1) close reason says it serialises the first build "of a scan".
- **Caches.** The Go frontend ignores a caller-supplied `GOCACHE`: `shatter-go/workspace/workspace.go:199-214` `GoEnv` replaces it with `<workspace>/cache/build`. The workspace root comes from `SHATTER_GO_WORKSPACE_ROOT` (`workspace.go:13`, `:86-116`), else `<repo>/.shatter/...`, else the user-data default. Compiled launchers are cached in the workspace's `BinariesDir` (`shatter-go/launcher/launcher.go:10-19`, `:201-203`). So "cold" means a fresh workspace root, not an empty `GOCACHE`.
- Audit run (finding goals-12, `audits/2026-09-22/goals-runs/go-all-default.err`), host load average 150-200: `Spawned 1 frontend session(s) for 18 target(s) (16 parallel worker(s))`, then `Error: explore: all 18 attempted target(s) failed (build_failed=0, runtime_failed=0, timed_out=43)`, each `request timed out after 30s`. The same explore with `-w 2 --request-timeout 180` completed. The verifier downgraded to P3: the load confounds the timeouts and it was not reproduced on a quiet host.

## Acceptance criteria

- [ ] `BuildWarmupGate` / `WarmupLeaderGuard` move to a shared module and are used by explore's multi-target worker pool on both paths (random `explorer.rs` and concolic `orchestrator.rs`), entered before each target's first execute. Scan keeps using the same type.
- [ ] **Deterministic ordering test** (shatter-core, no real frontend, no timing assumptions): drive the explore worker pool with several targets and a fake executor that records start/end events and blocks the first execute on a channel. Assert that no second target's first execute starts until the leader's first execute returns, and that after it returns at least two other targets' executes are in flight at once (fan-out). Also assert the gate opens when the leader fails. The test fails on current main (no gate) and passes on the branch; record the failing output. Run it once for each explorer path.
- [ ] **Real-frontend check** (Go E2E, `#[ignore]` like the rest of `e2e_concolic_go.rs`): explore several targets across multiple Go files with `SHATTER_GO_WORKSPACE_ROOT` set to a fresh empty temp dir (cold launcher and build caches) at default parallelism. Assert from the frontend log or a recorded event trace that exactly one harness build ran before any other started, then that builds overlapped. Record the host load average with the result in the close note.
- [ ] `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; paste the summary line showing `0 ignored` and the new test name). `task affected` passes with `Gates selected` recorded.

## Suggested approach

Lift the gate out of `scan_orchestrator.rs` into a shared module; have the explore worker pool `enter()` it before a target's first execute. Model the new ordering test on the three existing gate tests at `scan_orchestrator.rs:6634-6699`. Emit a debug-level event when the leader starts and ends so the E2E can assert order without timing.

## Out of scope

- Request/build timeout budgeting (`timeout-budget-invariant`).

## Dependencies

- Blocked by: none.
- Related: str-tbk9e (closed; scan-only fix), `timeout-budget-invariant`.

## Size

S-M

## References

- Finding goals-12 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/goals.md`). Old draft: `drafts/shatter-code/81-go-explore-warmup-gate.md`. Revised after the Codex cross-check (finding 11): an empty external `GOCACHE` does not produce a cold build, and a quiet-host pass could not distinguish the fix.
