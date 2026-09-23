---
slug: qwua7-49-rescope
kind: note-to-existing
title: "Note on str-qwua7.49: try_send/None premise refuted; real failure is an exhausted worker pool misreported as task timeouts"
priority: P2
type: bug
labels: [scan, worker-pool, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.49
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.49: try_send/None premise refuted; real failure is an exhausted worker pool misreported as task timeouts

**Target:** str-qwua7.49 (open). Add a comment and propose a retitle. No new issue.

## Comment text

> **Audit 2026-09-22 (finding core-10): the premise is refuted, so rescope.**
>
> **The original premise does not hold.** `scan_orchestrator.rs` contains no `try_send`. The pool-construction sites use `sender.send(fe).await.expect(...)` on a channel with capacity `max_workers` (`shatter-core/src/scan_orchestrator.rs:2237-2254`), so they cannot fail. `checkout()` (`:2268-2271`, `rx.recv().await.expect("pool should not be empty")`) cannot receive `None`, because `WorkerPool` owns its own sender, and shutdown drops it explicitly.
>
> **The real hazard.** `replace_dead_worker_if_needed` (`:2317-2327`) calls `reap_dead_slot()` when `Frontend::spawn` fails. The task loop then calls `pool.maybe_grow(remaining)` (`:4743`), which retries a detached spawn. If spawns keep failing (for example a broken frontend install), `live_count` reaches 0 while queued tasks block in `pool.checkout().await` (`:4607`). They stay blocked until `join_with_dynamic_watchdog` (`:5536`) fires, so a spawn failure is reported as a task timeout rather than a spawn error. The verifier confirmed the mechanism. It did not confirm that each task is then reported as `phase_timeout('task')`, so check that first.
>
> **Proposed rescope:**
> - Retitle: "Fail fast when the scan worker pool is exhausted".
> - Detect `live_count == 0 && pending > 0` and return `ScanError::WorkerPoolExhausted { last_spawn_error }` instead of letting checkouts block.
> - Add a regression test where every respawn fails, and assert that the new error is returned (with the spawn error text) and no task timeout is reported. It must fail on current `main` and pass after the fix.
> - Drop the `expect` → `?` conversion of the construction and checkout sites from scope, or keep it only as hygiene. Those sites are not reachable failure points.
>
> Line numbers verified at `56c86168`.
