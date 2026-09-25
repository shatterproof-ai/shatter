# NOTE on str-qwua7.50: poison-tolerant locks can't help — the handler has no panic boundary

| field | value |
|---|---|
| action | **append comment to existing issue `str-qwua7.50`** (no new issue) |
| suggested priority for target | P2 |
| source findings | frontend-rust-05 |

<!-- body -->
**Audit 2026-09-22 note** (findings: frontend-rust-05)



### Current code facts
- No `catch_unwind` in `shatter-rust/src/handler.rs:480-545` or `main.rs:24-30`; any panic ends the process (main exits 1), so a poisoned lock is never seen by a later request.
- Cache Mutexes (executor.rs:406, 526, 733) are accessed only from the dispatch thread; spawned threads (executor.rs:3055, 3206, 5475) only forward child stdout.

### Proposed changes / acceptance additions
- Re-scope: wrap dispatch in catch_unwind → internal_error response (clearing caches), then poison recovery; or replace the Mutexes with owned maps.
- Keep the module-docs half of the issue.
