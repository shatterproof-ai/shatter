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

str-tbk9e fixed a cold-cache thundering herd for **scan** by adding `BuildWarmupGate`: the first harness build of a run goes alone, then the other workers fan out once the build cache is warm. **Explore** never got the gate. A multi-target Go explore at default parallelism starts all workers on a cold `GOCACHE`, each triggers a full build at once, and under load they all hit the 30 s request timeout.

This is a parity gap between the scan and explore paths (see the "parallel parity" rule in the root CLAUDE.md). The user-facing failure was seen only on a heavily loaded host, so it is P3 until it is reproduced on a quiet one.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `grep -rn "BuildWarmupGate\|warmup_gate" shatter-core/src shatter-cli/src` finds hits only in `shatter-core/src/scan_orchestrator.rs` (struct at `:2138`, created at `:3870`, entered at `:3300`); none on the explore path (`shatter-cli/src/commands/explore.rs`, `shatter-core/src/explorer.rs`, `orchestrator.rs`).
- str-tbk9e (closed, P1) close reason says it serialises the first build "of a scan".
- Audit run (finding goals-12, `audits/2026-09-22/goals-runs/go-all-default.err`), host load average 150-200: `Spawned 1 frontend session(s) for 18 target(s) (16 parallel worker(s))`, then `Error: explore: all 18 attempted target(s) failed (build_failed=0, runtime_failed=0, timed_out=43)`, each `request timed out after 30s`. The same explore with `-w 2 --request-timeout 180` completed. The verifier downgraded to P3: the load confounds the timeouts and it was not reproduced on a quiet host.

## Acceptance criteria

- [ ] Explore uses the same warmup gate as scan (move `BuildWarmupGate` to a shared module and use it in both the random explorer and concolic orchestrator paths), or explore caps Go parallelism until the first harness build completes.
- [ ] Test: an E2E (Go) that explores several targets across multiple Go files with an empty `GOCACHE` and default parallelism succeeds. Run it on a quiet host (load average below the core count) and record the load and result in the close note; also record whether the pre-fix binary fails the same test on that host.
- [ ] `cargo test --test e2e_concolic_go` and `task affected` pass (`Gates selected` recorded).

## Suggested approach

Lift `BuildWarmupGate` and `WarmupLeaderGuard` out of `scan_orchestrator.rs` into a shared module; have the explore worker pool `enter()` the gate before a target's first execute. Check both explorer paths (random and concolic) per the parallel-parity rule.

## Out of scope

- Request/build timeout budgeting (tracked separately by the audit's timeout-budget issue).

## Dependencies

- Blocked by: none.
- Related: str-tbk9e (closed; scan-only fix).

## Size

S

## References

- Finding goals-12 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/goals.md`). Old draft: `drafts/shatter-code/81-go-explore-warmup-gate.md`.
