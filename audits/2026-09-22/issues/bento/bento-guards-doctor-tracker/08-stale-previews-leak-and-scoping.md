---
slug: stale-previews-leak-and-scoping
kind: new
title: "Stale land-work previews: bento test suite leaks previews into /tmp; default_preview_dir ignores TMPDIR; doctor warning is unscoped and names a closure mode that does not exist"
priority: P2
type: bug
labels: [audit, land-work, closure, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Stale land-work previews: bento test suite leaks previews into /tmp; default_preview_dir ignores TMPDIR; doctor warning is unscoped and names a closure mode that does not exist

Related: bento-rdtn.1, bento-7n7, bento-d91, bento-e583. Source findings: bento-09, sessions-17 (shatter audit 2026-09-22).

## Problem

1. Bento's own land-work tests leave preview worktrees in the shared `/tmp`.
2. `default_preview_dir()` hard-codes `/tmp`, so tests cannot isolate it through `TMPDIR`.
3. The doctor's stale-preview check globs every `/tmp/land-work-preview-*` from every repo and prints one line per directory.
4. The doctor says "let closure clean it up", but closure has no apply mode for previews.

## Evidence

- 2026-09-22: `/tmp` held 89-92 `land-work-preview-*` dirs, 86 created that day. Their `.git` files point at deleted `/tmp/tmpXXXX/repo` fixture repos, and their contents are bento fixture files (README.md, feature.txt, swarm-config.json, .land-work/).
- 2026-09-23 re-check: `ls -d /tmp/land-work-preview-* | wc -l` gives 130, of which 127 are newer than 2026-09-22. The leak continues.
- A simulated `collect_warnings` for the shatter worktree at now+2d produced 98 warnings, 89 of them "stale land-work preview".

## Current code facts (bento origin/main @ b1bb787, re-verified 2026-09-23)

- `catalog/skills/land-work/scripts/land-work-create-preview.py` lines 84-85: `default_preview_dir()` returns `Path(tempfile.mkdtemp(prefix="land-work-preview-", dir="/tmp")).resolve()`.
- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py` lines 791-812 (`check_stale_previews`): `tmp_root.glob("land-work-preview-*")`, no repo scoping, one warning per entry, wording "... remove it or let closure clean it up".
- `catalog/skills/closure/scripts/closure-scan.py` line 1660: `--apply` choices are `[APPLY_DELETE_LOCAL_MERGED, APPLY_DELETE_LOCAL_PATCH_EQUIVALENT]` only.

## Acceptance criteria

- `default_preview_dir` honours `TMPDIR` (`dir=None`, tempfile's default).
- Land-work tests set a per-test TMPDIR. A teardown assertion (or a conftest session fixture) fails if a test run created any `/tmp/land-work-preview-*`.
- The doctor reports only previews whose gitdir belongs to the current repo, collapsed to one line: "N stale land-work previews (oldest X days)".
- Either closure gains `--apply remove-stale-previews` (dry-run by default; skips previews with a live owner lock, see bento-e583), or the doctor wording drops the closure reference and gives the exact removal command.
- Proof at close: the close note shows the leak-detection fixture failing on the pre-fix code and passing after, and a full land-work test run leaving zero new `/tmp/land-work-preview-*` entries (before/after counts).

## Out of scope

- Preview owner locking between concurrent landers (bento-e583).
- Removing the previews already leaked on the maintainer's machine (a one-off manual cleanup).

## Priority / Type / Labels

P2 / bug / audit, land-work, closure, hygiene

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.
