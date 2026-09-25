---
slug: stale-previews-leak-and-scoping
kind: new
title: "Stale land-work previews: bento test suite leaks previews into /tmp; preview creation ignores TMPDIR; doctor discovery is unscoped, /tmp-only, and names a closure mode that does not exist"
priority: P2
type: bug
labels: [audit, land-work, closure, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Stale land-work previews: bento test suite leaks previews into /tmp; preview creation ignores TMPDIR; doctor discovery is unscoped, /tmp-only, and names a closure mode that does not exist

Related: bento-rdtn.1, bento-7n7, bento-d91, bento-e583 (open; preview owner lock between concurrent landers). Source findings: bento-09, sessions-17 (shatter audit 2026-09-22).

## Problem

1. Bento's own land-work tests leave preview worktrees in the shared `/tmp`.
2. `default_preview_dir()` hard-codes `/tmp`, so tests cannot isolate it through `TMPDIR`.
3. The doctor's stale-preview check globs every `/tmp/land-work-preview-*` from every repo and prints one line per directory. It looks only in `/tmp` (`main()` defaults `tmp_root` to `Path("/tmp")`), so once creation honours `TMPDIR`, previews created elsewhere would silently drop out of discovery.
4. The doctor says "let closure clean it up", but closure has no apply mode for previews.

## Evidence

- 2026-09-22: `/tmp` held 89-92 `land-work-preview-*` dirs, 86 created that day. Their `.git` files point at deleted `/tmp/tmpXXXX/repo` fixture repos, and their contents are bento fixture files (README.md, feature.txt, swarm-config.json, .land-work/).
- 2026-09-23 re-checks: 130, then 132 `land-work-preview-*` dirs, almost all newer than 2026-09-22. The leak continues.
- A simulated `collect_warnings` for the shatter worktree at now+2d produced 98 warnings, 89 of them "stale land-work preview".

## Current code facts (bento origin/main @ 0b8d488; cited files unchanged since b1bb787)

- `catalog/skills/land-work/scripts/land-work-create-preview.py` lines 84-85: `default_preview_dir()` returns `Path(tempfile.mkdtemp(prefix="land-work-preview-", dir="/tmp")).resolve()`.
- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py` lines 791-812 (`check_stale_previews`): `tmp_root.glob("land-work-preview-*")`, no repo scoping, one warning per entry, wording "... remove it or let closure clean it up". Line 1191: `resolved_tmp_root = tmp_root if tmp_root is not None else Path("/tmp")`. The Codex doctor has its own copy.
- `catalog/skills/closure/scripts/closure-scan.py` line 1660: `--apply` choices are `[APPLY_DELETE_LOCAL_MERGED, APPLY_DELETE_LOCAL_PATCH_EQUIVALENT]` only.

## Acceptance criteria

- **Creation.** `default_preview_dir` uses `tempfile.mkdtemp(prefix="land-work-preview-")` with no `dir=`, so it honours `TMPDIR`.
- **Discovery agrees with creation.** Both doctors find previews in two ways and de-duplicate by real path:
  - owned previews: registered worktrees of the current repo (`git worktree list --porcelain`) whose directory name starts with `land-work-preview-`, wherever they live;
  - unowned previews: `land-work-preview-*` under both `tempfile.gettempdir()` and `/tmp` whose `.git` file points at a gitdir that no longer exists.
  Test: a stale preview of the current repo created under a non-`/tmp` `TMPDIR` is reported; one under `/tmp` still is.
- **Output.** Owned stale previews collapse to one line, "N stale land-work previews for this repo (oldest X days)". Unowned previews collapse to one separate line, "N orphaned land-work previews whose repo no longer exists", shown at most once per day (seen-state throttled). Previews of other live repos are not reported. Tests cover each.
- **Test hygiene.** Land-work tests set a per-test `TMPDIR`. A conftest session fixture records `/tmp/land-work-preview-*` before and after the session and fails if any were added.
- **No reference to a nonexistent mode.** Either closure gains `--apply remove-stale-previews` (dry-run by default; removes only previews with no live owner per bento-e583's lock, and only via `git worktree remove` for owned previews), or the doctor wording drops the closure reference and prints the exact removal commands. A test asserts the doctor never names an `--apply` mode that `closure-scan.py --help` does not list.
- Proof at close: the close note shows the leak-detection fixture failing on the pre-fix code and passing after, and a full land-work test run leaving zero new `/tmp/land-work-preview-*` entries (before/after counts).

## Out of scope

- Preview owner locking between concurrent landers (bento-e583).
- Removing the previews already leaked on the maintainer's machine (a one-off manual cleanup).

## Priority / Type / Labels

P2 / bug / audit, land-work, closure, hygiene

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None. If the closure apply mode is chosen, it must use bento-e583's owner lock once that lands.
