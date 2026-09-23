---
slug: e583-preview-owner-lock
kind: note-to-existing
title: "Note on bento-e583: confirmed mechanism (the leftover-preview refusal has no owner check); add an owner lock, --force-foreign, and a two-process test"
priority: P1
type: note
labels: [audit, land-work, concurrency]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-e583
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-e583: confirmed mechanism, owner lock, --force-foreign, two-process test

Target: **bento-e583** (open, P1, "land-work-preview-* worktree destroyed mid-verification by concurrent session"). Post as a comment. Do not file a new issue. Leave the priority at P1.

Comment text:

> **Audit 2026-09-22 (shatter), finding bento-01: mechanism confirmed**
>
> The stale-preview refusal added by bento-rdtn.3 has no owner check. Its "remove them first" hint tells a lander to delete another lander's live preview. Shatter sessions followed that hint and destroyed each other's in-flight landings.
>
> **Code (re-verified 2026-09-23 at bento origin/main b1bb787; land-work scripts unchanged since 1c0c1e6), `catalog/skills/land-work/scripts/`:**
> - `land-work-create-preview.py:62-81`: `leftover_preview_worktrees()` returns every registered worktree whose name starts with `land-work-preview-`. It checks no pid, lock, session or age.
> - `land-work-create-preview.py:252-275`: when any leftovers exist, it fails with `leftover land-work-preview-* worktree(s) exist: ...; remove them first (land-work-create-preview.py --cleanup --preview-dir <p>) or pass --allow-existing`.
> - `land-work-create-preview.py:163-173`: `cleanup_preview()` runs `git worktree remove --force` on any path it is given, with no owner check.
> - `land.py:291`: `create_preview` is called with only `--base-ref`. It never passes `--allow-existing`, so every concurrent landing in the same repo trips the refusal.
>
> **Transcript evidence (shatter, 2026-09-20/21):**
> - 23:49: session 87606e10 got `create_preview: failed` with "leftover ... /tmp/land-work-preview-mz7t9g27; remove them first". `git status` inside that preview showed a merge in progress, meaning it was live. The session ran `--cleanup` on it. The owning session 9f13ca23 then reported `verify: failed (238.362s)` and `cleanup: failed`.
> - 23:54-23:55: the same thing in the other direction (previews d1bfx2yj and g663hhnu, this time removed with `rm -rf`).
> - 00:00: a session hit `FileNotFoundError` on its own preview 2vu7job9.
>
> **Proposed additions to acceptance criteria:**
> 1. create-preview writes an owner record (`.land-work/owner.json`: pid, hostname, session id when known, start time, branch) into each scratch preview. land.py holds an `fcntl.flock` on it for the whole run.
> 2. `leftover_preview_worktrees()` skips a preview whose lock is held or whose recorded pid is alive on this host. When only such previews exist, it proceeds, or, if a repo-wide landing lease is wanted, fails with "another landing is in progress (pid N, branch B); wait". Neither message says "remove".
> 3. Only previews that are unlocked, have a dead pid, and are older than a grace period count as leftovers.
> 4. `--cleanup` refuses a preview it does not own (lock held by another process, or recorded pid alive and not the caller's) unless `--force-foreign` is passed. The refusal names the owner.
> 5. Regression test with two real processes: process A creates a preview and holds it (for example through the existing `BENTO_LAND_TEST_DELAY_*` style seam), and process B runs create-preview and `--cleanup` against it. A's preview must still exist and A must finish. Commit the test failing against the current code first, then passing.
>
> **Proof at close:** the two-process test's name and a failing-then-passing run (`python3 -m unittest tests.land_work.<module>`), not "merged".
>
> Related: bento-rdtn.3 (closed; introduced the refusal), bento-7n7 and bento-gd2 (closed; preview leaks), `stale-previews-leak-and-scoping` (same epic; leak and scoping side), `dyp7-admission-control` (host load).
