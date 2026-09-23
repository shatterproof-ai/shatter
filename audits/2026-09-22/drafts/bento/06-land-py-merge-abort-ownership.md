# land.py aborts or hard-resets merge state in the shared primary checkout that it did not start

- Filing action: new issue
- Priority: P2
- Type: bug
- Labels: audit, land-work, safety
- Parent: epic
- Source findings: bento-05

---BODY---
## Problem

On any failure or signal, land.py runs `git merge --abort` in the primary checkout whenever `.git/MERGE_HEAD` exists. That includes failures at the prepare, create_preview and verify steps, before land.py itself has touched the primary. The tree-mismatch recovery path runs `git reset --hard HEAD@{1}`, which is wrong if anything else moved HEAD. In repos where several sessions share one primary checkout (shatter), this can destroy another session's in-progress merge.

This was found by code reading. No collision has been observed.

## Current code facts (bento @ 1c0c1e6, catalog/skills/land-work/scripts/land.py)

- `abort_primary_merge_if_in_progress()` (about lines 144-148) aborts whenever MERGE_HEAD exists.
- It is called from the StepFailure, BaseException and SIGINT/SIGTERM paths (about lines 348-379).
- Line 204: `subprocess.run(["git", "reset", "--hard", "HEAD@{1}"], cwd=primary_root, ...)`.

## Acceptance criteria

- land.py records `merge_started=True` and the pre-merge SHA when it begins the primary merge. Abort and reset happen only when `merge_started` is true, and the reset targets the recorded SHA.
- Test: a primary with a pre-existing MERGE_HEAD plus a land.py failure at verify leaves MERGE_HEAD untouched.
- Test: a failure after land.py's own merge restores the recorded SHA.

## Out of scope

- Other locking between sessions. See the admission-control and preview-lock issues (bento-e583, bento-dyp7).
