# NOTE TO APPEND to bento-e583 (duplicate-open; not a new issue)

---BODY---
## Confirmed mechanism (shatter audit 2026-09-22, finding bento-01)

The stale-preview refusal from bento-rdtn.3 has no owner check, and its "remove them first" hint sends a lander to delete another lander's live preview.

Current code facts (bento @ 1c0c1e6):
- In `catalog/skills/land-work/scripts/land-work-create-preview.py`, `leftover_preview_worktrees()` (about lines 62-81) flags every registered `land-work-preview-*`. It checks no owner, pid or lock.
- The error text (about lines 266-276) says "remove them first (... --cleanup ...)".
- `land.py:291` never passes `--allow-existing`.

Transcript evidence (shatter, 2026-09-20):
- 23:49: session 87606e10 hit "leftover ... /tmp/land-work-preview-mz7t9g27; remove them first". `git status` there showed an in-progress merge. It ran `--cleanup`.
- The owning session 9f13ca23 then got "verify: failed (238.362s) / cleanup: failed".
- The same thing happened in reverse at 23:54-23:55 (d1bfx2yj, g663hhnu, with `rm -rf`).
- 00:00: FileNotFoundError on the session's own preview 2vu7job9.

Additions to acceptance:
- Write an owner lockfile (pid, session, start) into each preview and hold an flock for the whole land.py run.
- The leftover check skips previews that are locked or whose pid is alive, and says "another landing is in progress; wait".
- `--cleanup` refuses previews it does not own unless `--force-foreign` is given.
- Add a two-process regression test.
