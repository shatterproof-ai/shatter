---
slug: land-py-merge-abort-ownership
kind: new
title: "land.py aborts or hard-resets merge state in the shared primary checkout even when it did not start the merge"
priority: P2
type: bug
labels: [audit, land-work, safety, concurrency]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# land.py aborts or hard-resets merge state in the shared primary checkout even when it did not start the merge

## Problem

On any failure or signal, land.py runs `git merge --abort` in the primary checkout whenever `.git/MERGE_HEAD` exists. This happens even for failures at `prepare`, `create_preview` or `verify`, before land.py has touched the primary. Separately, the tree-mismatch recovery path runs `git reset --hard HEAD@{1}`, which resets to the wrong commit if another session moved HEAD between land.py's merge and the reset. In repos where several sessions share one primary checkout, as in shatter, either action can destroy another session's in-progress merge or commits.

This was found by reading the code. No collision has been observed yet. It is the same class of failure as bento-e583 (acting on shared state without an ownership check).

## Evidence

Re-verified 2026-09-23 at bento origin/main 0b8d488 (land-work unchanged since 1c0c1e6). File: `catalog/skills/land-work/scripts/land.py`.

- `:144-148`: `abort_primary_merge_if_in_progress()` runs `git merge --abort` whenever `self.primary_root/.git/MERGE_HEAD` exists. It does not check who created it.
- It is called from the signal handler (`:348-350`), the `except StepFailure` handler (`:366-368`) and the `except BaseException` handler (`:376-378`). `self.primary_root` is set right after `prepare` (`:278`), so a failure in `create_preview` or `verify` still reaches the abort.
- `land-work-prepare.py:117-121` refuses a dirty primary checkout, so a merge that was **already** in progress before land.py started normally stops the run at `prepare`, before `primary_root` is set. The realistic hazard is a merge that another session starts in the primary checkout **after** land.py's prepare, while land.py is in `create_preview`, `verify` or `lease_check` (several minutes in shatter).
- `:196-199`: after land.py's own failed `git merge`, the abort is correct.
- `:200-204`: on tree mismatch, `subprocess.run(["git", "reset", "--hard", "HEAD@{1}"], cwd=primary_root, ...)`. `HEAD@{1}` is the previous reflog entry. After land.py's own fast-forward (`:177-185`) and merge, `HEAD@{1}` is exactly the pre-merge SHA, so the fast-forward alone is not a defect. It is wrong only when another session moves HEAD in the primary checkout between the merge and the reset.
- `:187-192`: the existing test seam `BENTO_LAND_TEST_DELAY_MERGE` sleeps **before** `git merge` runs, so the existing SIGINT test never has a merge in progress when the signal arrives. It cannot show that a real in-progress merge is aborted.

## Acceptance criteria

Ownership rule. A flag set in land.py's own process ("I attempted a merge") is not proof of ownership: another session could start a merge after land.py's prepare, and land.py would then abort it. Ownership must be established under serialization:

- [ ] land.py serializes its primary-checkout mutation with other bento landers: it takes an exclusive `fcntl.flock` on `<git-common-dir>/bento/primary-checkout.lock` immediately before the fast-forward/merge in `_merge_in_primary` and holds it until the push has succeeded or the merge has been undone. (Other land-work helpers that mutate the primary checkout take the same lock; bento-e583's preview lock is separate.)
- [ ] Under that lock, land.py checks that `MERGE_HEAD` does not exist and records `pre_merge_sha = HEAD`. If `MERGE_HEAD` already exists it stops with "another merge is in progress in the primary checkout" and touches nothing.
- [ ] `abort_primary_merge_if_in_progress()` aborts only when land.py holds the lock, recorded that it started the merge, **and** `MERGE_HEAD` contains the SHA land.py merged (the feature head). Otherwise it leaves the merge state alone and reports it in the failure JSON (`primary_merge_left_untouched: true`).
- [ ] The tree-mismatch path resets to the recorded `pre_merge_sha`, never to `HEAD@{1}`, and only while holding the lock and after confirming that HEAD is still land.py's own merge commit (first parent `pre_merge_sha`, second parent the feature head). If that check fails it refuses to reset and reports why. The remaining window (a non-bento process mutating the primary checkout without taking the lock) is documented in the code comment as out of scope.

Tests (`tests/land_work/test_land_driver.py`):

- [ ] **Foreign merge after prepare.** Using a seam that runs after `prepare` (for example a new `BENTO_LAND_TEST_AFTER_PREPARE` hook command, or the verifier stub itself), another process starts `git merge --no-commit` of an unrelated branch in the primary checkout; land.py's verify then fails. After land.py exits, the foreign `MERGE_HEAD`, index and working tree are unchanged. This test is committed failing against the current code first (today land.py aborts the foreign merge), then passing.
- [ ] **Signal during land.py's own merge.** A seam that pauses **while** land.py's merge is in progress (for example a `pre-merge-commit` hook in the fixture repo that sleeps, or a new seam between `git merge --no-commit` and the commit). The test first asserts that `MERGE_HEAD` exists at the pause point, then sends SIGINT, then asserts the merge was aborted and HEAD equals `pre_merge_sha`. The old `BENTO_LAND_TEST_DELAY_MERGE` test stays but is not counted as proof of abort.
- [ ] **Concurrent HEAD move before reset.** Force a tree mismatch and, through a seam between the merge and the reset, add a commit on top of land.py's merge in the primary checkout from another process. land.py refuses to reset and reports it; the extra commit survives. (A plain fast-forward-then-mismatch case is not a regression test: current code already restores the right SHA there.)
- [ ] **Existing merge refused.** With the lock free but a foreign `MERGE_HEAD` present at merge time, land.py stops with the "another merge is in progress" error and leaves it untouched.
- [ ] Proof at close: the four test names, with the foreign-merge and concurrent-HEAD tests shown failing against the current code and then passing (`python3 -m unittest tests.land_work.test_land_driver`), in the close reason. "Merged" is not sufficient.

## Suggested approach

Store `merge_started`, `merged_head` and `pre_merge_sha` on `Driver` and hold the lock file descriptor there as well. Release the lock in the same `finally` path that runs cleanup.

## Out of scope

- Locking between sessions beyond the primary-checkout merge window: preview ownership is bento-e583, host admission is bento-dyp7.
- The `_push_from_preview` route, which never merges in the primary.

## Dependencies

- Blocked by: none.
- Related: bento-e583, bento-dyp7, bento-rdtn.14 (closed).

Priority: P2 · Type: bug · Labels: audit, land-work, safety, concurrency · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/06, bento-05
