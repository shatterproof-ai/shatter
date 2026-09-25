# NOTE on str-qwua7.49: premise is wrong; real failure is an empty worker pool misreported as task timeouts

| field | value |
|---|---|
| action | **append comment to existing issue `str-qwua7.49`** (no new issue) |
| suggested priority for target | P2 |
| source findings | core-10 |

<!-- body -->
**Audit 2026-09-22 note** (findings: core-10)

Rescope/retitle proposal for str-qwua7.49.

### Current code facts
- The cited sites use `sender.send(fe).await.expect` (scan_orchestrator.rs:2240-2254) on a max_workers-capacity channel, not try_send; `checkout()` recv cannot yield None because WorkerPool owns a sender.
- Real hazard: `replace_dead_worker_if_needed` → `reap_dead_slot` on spawn failure (:2317-2327, :4608) then `maybe_grow(remaining)` (:4743) retries detached; if spawns keep failing, live_count hits 0 while tasks block in `pool.checkout().await` until the dynamic watchdog fires (reported as a task timeout — unverified).

### Proposed changes / acceptance additions
- Retitle to 'Fail fast when the scan worker pool is exhausted'.
- Detect live_count==0 with pending>0 and return `ScanError::WorkerPoolExhausted{last_spawn_error}`.
- Regression test where every respawn fails asserts the new error, not a timeout.
