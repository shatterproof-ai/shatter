# Stale land-work previews: bento test suite leaks ~90 into /tmp; default_preview_dir ignores TMPDIR; doctor warning unscoped and names a closure mode that does not exist

- Filing action: new issue
- Priority: P2
- Type: bug
- Labels: audit, land-work, closure, hygiene
- Parent: epic
- Links: related bento-rdtn.1, related bento-7n7, related bento-d91
- Source findings: bento-09, sessions-17

---BODY---
## Problem

1. Bento's own land-work tests leave preview worktrees in the shared `/tmp`.
2. `default_preview_dir()` hard-codes `/tmp`, so tests cannot isolate it through `TMPDIR`.
3. The doctor's stale-preview check globs every `/tmp/land-work-preview-*` from every repo and prints one line per directory.
4. The doctor's wording says "let closure clean it up", but closure has no apply mode for previews.

## Evidence (2026-09-22)

- `/tmp` holds 89-92 `land-work-preview-*` dirs, 86 of them created on 09-22.
- Their `.git` files point at deleted `/tmp/tmpXXXX/repo` fixture repos, and their contents are bento fixture files (README.md, feature.txt, swarm-config.json, .land-work/).
- A simulated `collect_warnings` for the shatter worktree at now+2d produced 98 warnings, 89 of them "stale land-work preview".

## Current code facts (bento @ 1c0c1e6)

- `catalog/skills/land-work/scripts/land-work-create-preview.py:84-85`: `default_preview_dir()` returns `Path(tempfile.mkdtemp(prefix="land-work-preview-", dir="/tmp"))`.
- `agent-env-doctor.py` about lines 791-810 (`check_stale_previews`) globs the tmp root, does no repo scoping and emits one warning per entry.
- `catalog/skills/closure/scripts/closure-scan.py:1660`: `--apply` choices are `[APPLY_DELETE_LOCAL_MERGED, APPLY_DELETE_LOCAL_PATCH_EQUIVALENT]` only.

## Acceptance criteria

- `default_preview_dir` honours `TMPDIR` (`dir=None` uses tempfile's default).
- Land-work tests set a per-test TMPDIR. A teardown assertion (or a conftest session fixture) fails if a test run created any `/tmp/land-work-preview-*`.
- The doctor reports only previews whose gitdir belongs to the current repo, and collapses them to a single line: "N stale land-work previews (oldest X days)".
- Either closure gains `--apply remove-stale-previews` (dry-run by default; skips previews with a live owner lock, see bento-e583), or the doctor wording drops the closure reference and gives the exact removal command.

## Out of scope

- Preview owner locking between concurrent landers (bento-e583).
