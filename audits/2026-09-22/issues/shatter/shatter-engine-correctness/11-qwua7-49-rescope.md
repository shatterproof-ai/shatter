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
> **The real hazard.** `replace_dead_worker_if_needed` (`:2317-2327`) calls `reap_dead_slot()` when `Frontend::spawn` fails. After each task, the task loop calls `pool.maybe_grow(remaining)` (`:4743`). `maybe_grow` (`:2336-2363`) increments `live_count` before it starts a detached spawn, and decrements it again if that spawn fails. If spawns keep failing (for example a broken frontend install), `live_count` can reach 0 while queued tasks block in `pool.checkout().await` (`:4608`). Once no task is running, nothing calls `maybe_grow` again, and the blocked checkouts wait until `join_with_dynamic_watchdog` (`:5536`) fires. A spawn failure is then reported as a timeout rather than a spawn error. The verifier confirmed the mechanism. It did not confirm that each task is then reported as `phase_timeout('task')`; the first step of this work is to reproduce that and record the actual reported outcome.
>
> **Proposed rescope:**
> - Retitle: "Fail fast when the scan worker pool is exhausted".
> - Define exhaustion so a successful retry is never rejected: the pool is exhausted only when there are no live workers, **no spawn attempts in flight** (from `maybe_grow` or `replace_dead_worker_if_needed`), and tasks are still waiting. A plain `live_count == 0 && pending > 0` check is not enough: `maybe_grow` counts an in-flight spawn in `live_count` before it succeeds or fails, and a failed replacement can make `live_count` 0 for a moment while another spawn is still about to succeed. Track in-flight spawns explicitly, and keep the last spawn error.
> - When exhaustion is detected (at the point where the last in-flight spawn fails), wake every blocked `checkout()` (for example by closing the channel or signalling a `Notify`), and have `checkout()` return `Err(ScanError::WorkerPoolExhausted { last_spawn_error })` instead of panicking or waiting.
> - Regression tests, each failing on current `main` and passing after the fix, with output quoted in the close note:
>   1. Every spawn after the initial pool fails: the scan returns `WorkerPoolExhausted` containing the spawn error text, within a bound far below the watchdog timeout, and no task timeout is reported.
>   2. Transient failure: the first replacement spawn fails and the next succeeds. All tasks complete, and no `WorkerPoolExhausted` is returned.
>   3. Checkouts blocked at the moment of exhaustion are woken and receive the error (no task is left waiting).
> - Drop the `expect` → `?` conversion of the construction and checkout sites from scope, or keep it only as hygiene. Those sites are not reachable failure points.
> - The existing acceptance item "checkpoint and partial report still written, exit 2 at the CLI" still applies to the new error path.
>
> Line numbers verified on the audit branch (code identical to `56c86168`).
