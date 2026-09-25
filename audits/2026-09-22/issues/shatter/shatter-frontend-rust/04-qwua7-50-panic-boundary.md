---
slug: qwua7-50-panic-boundary
kind: note-to-existing
title: "NOTE on str-qwua7.50: poison-tolerant locks cannot help in production because the Rust handler has no panic boundary"
priority: P2
type: bug
labels: [rust-frontend, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.50
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# NOTE on str-qwua7.50: poison-tolerant locks cannot help in production because the Rust handler has no panic boundary

Target: **str-qwua7.50** (open). Action: post the comment below with `bd comments add str-qwua7.50 ...`. Do not change the priority.

## Comment text

> **Audit 2026-09-22 note** (finding frontend-rust-05). This re-scopes the lock half of this issue.
>
> **Current code (re-verified at main 16794cef):**
> - `shatter-rust/src/handler.rs:480` (`Handler::run`) and `:529` (`dispatch`) have no `catch_unwind`, and neither does `shatter-rust/src/main.rs`. The only `catch_unwind` calls are in generated harness code and the generators' native invoke path. A panic while handling a request therefore ends the frontend process (main exits non-zero). No later request in production ever sees a poisoned lock.
> - The cache Mutexes (`executor.rs:406` `CrateHarnessCache`, `:526` `CrateBridgeHarnessCache`, `:733` `HarnessCache`) are only accessed from the dispatch thread. The threads spawned at `executor.rs` ~3055, ~3206, ~5475 only forward child stdout and never touch the caches.
>
> **Correction to the acceptance test as written.** A unit test *can* exercise poison recovery by wrapping a handler call in `catch_unwind` itself and then reusing the handler, so the test is not impossible. But it would pass without fixing anything real, because production cannot survive a panic without a handler-level boundary.
>
> **Proposed re-scope (replaces the poison-tolerant-lock acceptance item):**
> - **Required:** a panic boundary. Wrap `dispatch` in `std::panic::catch_unwind(AssertUnwindSafe(..))`, turn a caught panic into an `internal_error` response naming the panic message, and invalidate (clear) every harness cache the panicking request could have touched. Without this boundary the process dies on the first panic and nothing else in this issue matters.
> - **Then, either** keep the Mutexes and route all 19 lock sites through the `lock_cache` recover-and-clear helper this issue already specifies, **or** replace the Mutexes with owned `HashMap`s on the handler (access is single-threaded). The owned-map option removes poisoning but does not by itself keep the process alive, so it is only acceptable together with the panic boundary above; its cache invalidation happens in the boundary's catch arm.
> - Acceptance: an integration test that spawns **one** frontend process and, over the stdio protocol, sends a request that forces a panic inside dispatch (e.g. a test-only hook or a malformed input known to hit an `expect`), then a normal execute request. The first gets `internal_error` with the panic message, the second succeeds, and the process is still alive afterwards. Show it failing on main (process exits, second request gets no response) and passing on the branch; paste both into the close note.
> - Keep the module-docs half of this issue and the Option::unwrap cleanup unchanged.
