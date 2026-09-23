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

On any failure or signal, land.py runs `git merge --abort` in the primary checkout whenever `.git/MERGE_HEAD` exists. This happens even for failures at `prepare`, `create_preview` or `verify`, before land.py has touched the primary. Separately, the tree-mismatch recovery path runs `git reset --hard HEAD@{1}`, which resets to the wrong commit if anything else moved HEAD in the meantime. In repos where several sessions share one primary checkout, as in shatter, either action can destroy another session's in-progress merge or commits.

This was found by reading the code. No collision has been observed yet. It is the same class of failure as bento-e583 (acting on shared state without an ownership check).

## Evidence

Re-verified 2026-09-23 at bento origin/main b1bb787 (unchanged since 1c0c1e6). File: `catalog/skills/land-work/scripts/land.py`.

- `:144-148`: `abort_primary_merge_if_in_progress()` runs `git merge --abort` whenever `self.primary_root/.git/MERGE_HEAD` exists. It does not check who created it.
- It is called from the signal handler (`:348-350`), the `except StepFailure` handler (`:365-368`) and the `except BaseException` handler (`:376-378`). `self.primary_root` is set right after `prepare` (`:278`), so a failure in `create_preview` or `verify` still reaches the abort.
- `:196-199`: after land.py's own failed `git merge`, the abort is correct.
- `:200-204`: on tree mismatch, `subprocess.run(["git", "reset", "--hard", "HEAD@{1}"], cwd=primary_root, ...)`. `HEAD@{1}` is the previous reflog entry. It is only the pre-merge SHA if nothing else moved HEAD, and in the `:177-185` path land.py has itself just fast-forwarded, which adds its own reflog entry.

## Acceptance criteria

- [ ] land.py records `merge_started = True` and `pre_merge_sha = rev_parse("HEAD", primary_root)` immediately before its own `git merge` in `_merge_in_primary`. `abort_primary_merge_if_in_progress()` does nothing unless `merge_started` is true.
- [ ] The tree-mismatch path resets to the recorded `pre_merge_sha` and never to `HEAD@{1}`. Before resetting, it checks that HEAD is still land.py's own merge commit. If it is not, it refuses to reset and reports the situation.
- [ ] Test: a primary checkout with a pre-existing `MERGE_HEAD`, plus a land.py failure injected at `verify`, leaves `MERGE_HEAD` and the index untouched.
- [ ] Test: a SIGINT during land.py's own merge (existing `BENTO_LAND_TEST_DELAY_MERGE` seam) still aborts, which preserves current behaviour.
- [ ] Test: a forced tree mismatch after a fast-forward restores exactly the recorded SHA.
- [ ] Proof at close: the three test names, with the first test shown failing against the current code and then passing (`python3 -m unittest tests.land_work.test_land_driver`). "Merged" is not sufficient.

## Suggested approach

Store the two fields on `Driver`. Set them inside `_merge_in_primary` just before the `git merge --no-ff` call at `:193`. Clear `merge_started` once the push succeeds.

## Out of scope

- Locking between sessions more generally: preview ownership is bento-e583, host admission is bento-dyp7.
- The `_push_from_preview` route, which never merges in the primary.

## Dependencies

- Blocked by: none.
- Related: bento-e583, bento-dyp7, bento-rdtn.14 (closed).

Priority: P2 · Type: bug · Labels: audit, land-work, safety, concurrency · Parent: Epic: Audit 2026-09-22 findings (bento) · Sources: bento/06, bento-05
