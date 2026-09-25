---
slug: e583-preview-owner-lock
kind: note-to-existing
title: "Note on bento-e583: confirmed mechanism (the leftover-preview refusal has no owner check); add an owner lock with a token handoff to the cleanup subprocess, --force-foreign, and a two-process test"
priority: P1
type: note
labels: [audit, land-work, concurrency]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-e583
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-e583: confirmed mechanism, owner lock with token handoff, --force-foreign, two-process test

Target: **bento-e583** (open, P1, "land-work-preview-* worktree destroyed mid-verification by concurrent session"). Post as a comment. Do not file a new issue. Leave the priority at P1.

Comment text:

> **Audit 2026-09-22 (shatter), finding bento-01: mechanism confirmed**
>
> The stale-preview refusal added by bento-rdtn.3 has no owner check. Its "remove them first" hint tells a lander to delete another lander's live preview. Shatter sessions followed that hint and destroyed each other's in-flight landings.
>
> **Code (re-verified 2026-09-23 at bento origin/main 0b8d488; land-work scripts unchanged since 1c0c1e6), `catalog/skills/land-work/scripts/`:**
> - `land-work-create-preview.py:60-79`: `leftover_preview_worktrees()` returns every registered worktree whose name starts with `land-work-preview-`. It checks no pid, lock, session or age.
> - `land-work-create-preview.py:252-275`: when any leftovers exist, it fails with `leftover land-work-preview-* worktree(s) exist: ...; remove them first (land-work-create-preview.py --cleanup --preview-dir <p>) or pass --allow-existing`.
> - `land-work-create-preview.py:163-173`: `cleanup_preview()` runs `git worktree remove --force` on any path it is given, with no owner check.
> - `land.py:291`: `create_preview` is called with only `--base-ref`. It never passes `--allow-existing`, so every concurrent landing in the same repo trips the refusal.
> - `land.py:129-142`: land.py's own cleanup runs `land-work-create-preview.py --cleanup --preview-dir <p>` as a **separate subprocess**. Any ownership rule therefore has to let that child act for its parent; a plain "pid/lock belongs to someone else" test would refuse land.py's own cleanup.
>
> **Transcript evidence (shatter, 2026-09-20/21):**
> - 23:49: session 87606e10 got `create_preview: failed` with "leftover ... /tmp/land-work-preview-mz7t9g27; remove them first". `git status` inside that preview showed a merge in progress, meaning it was live. The session ran `--cleanup` on it. The owning session 9f13ca23 then reported `verify: failed (238.362s)` and `cleanup: failed`.
> - 23:54-23:55: the same thing in the other direction (previews d1bfx2yj and g663hhnu, this time removed with `rm -rf`).
> - 00:00: a session hit `FileNotFoundError` on its own preview 2vu7job9.
>
> **Proposed additions to acceptance criteria:**
> 1. **Owner record and lock.** create-preview writes `.land-work/owner.json` into each scratch preview: a random owner token, pid, hostname, session id when known, start time, branch. It prints the token in its JSON payload. The driver that created the preview (land.py, or an agent following the manual flow) holds an `fcntl.flock` on `.land-work/owner.lock` for as long as it uses the preview.
> 2. **Authenticated handoff to the cleanup subprocess.** `--cleanup` accepts `--owner-token <t>` (or reads it from `BENTO_LAND_OWNER_TOKEN`, which land.py sets for its children). A cleanup whose token matches `owner.json` is the owner acting through a child process and proceeds even though the parent holds the lock. The manual flow in SKILL.md passes the token printed by create-preview.
> 3. **Foreign cleanup refused.** `--cleanup` without a matching token refuses while the lock is held or the recorded pid is alive on this host, unless `--force-foreign` is passed. The refusal names the owner (pid, branch, start time).
> 4. **Leftover detection.** `leftover_preview_worktrees()` skips any preview whose lock is held or whose recorded pid is alive on this host. Only previews that are unlocked, have a dead (or absent) owner pid and are older than a grace period count as leftovers. When only live previews exist, create-preview proceeds; if a repo-wide landing lease is wanted instead, it fails with "another landing is in progress (pid N, branch B); wait". Neither message tells the lander to remove anything.
> 5. **Tests.**
>    - Two real processes: process A (land.py, paused through a `BENTO_LAND_TEST_DELAY_*` style seam while it holds its preview) and process B running create-preview and then `--cleanup` against A's preview without the token. A's preview still exists, B's cleanup exits non-zero naming A, and A finishes with `ok: true`.
>    - Owner cleanup: an unmodified land.py run (success and injected verify failure) still records `cleanup: passed` and leaves no registered preview, i.e. the token handoff lets land.py's own cleanup subprocess through while the lock is held.
>    - `--force-foreign` removes a live foreign preview.
>    - The two-process test is committed failing against the current code first, then passing.
>
> **Proof at close:** the three test names, and the two-process test's failing-then-passing run (`python3 -m unittest tests.land_work.test_land_driver tests.land_work.test_land_work_scripts`), pasted into the close reason. "Merged" is not sufficient.
>
> Related: bento-rdtn.3 (closed; introduced the refusal), bento-7n7 and bento-gd2 (closed; preview leaks), `stale-previews-leak-and-scoping` (same epic; leak and scoping side), `dyp7-admission-control` (host load).
