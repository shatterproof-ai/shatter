---
slug: advise-taxonomy-payload
kind: new
title: "shatter-advise cites an unshipped taxonomy spec and shatter-gaps references the taxonomy with no resolvable source; stale agents-arz id; test file shipped in the payload"
priority: P2
type: bug
labels: [skills, packaging, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# shatter-advise cites an unshipped taxonomy spec and shatter-gaps references the taxonomy with no resolvable source; stale agents-arz id; test file shipped in the payload

## Problem

shatter-advise and shatter-gaps produce findings that cite named patterns by `pattern_id`. shatter-advise says those patterns are defined in `docs/specs/2026-06-16-shatter-tractability-taxonomy.md` "(in the shatter-agents repo)". That file exists only in the source repo and is not part of either published payload. shatter-gaps refers to "the Shatter tractability taxonomy" and emits `<pattern_id>` values but cites no file at all. An installed agent therefore cannot read the catalog its own findings cite.

The skill also references the tracker ids `agents-arz` (the live issue is `sa-arz`, deferred) and `agents-2b3`. Tracker ids do not belong in shipped skill text in any case.

Finally, `scripts/build-plugins` copies `scripts/test_discover_hotspots.py` into the published payload, because its ignore rule covers only `__pycache__` and `.pyc`.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807`.

- `catalog/skills/shatter-advise/SKILL.md:317` reads "The taxonomy spec at `docs/specs/2026-06-16-shatter-tractability-taxonomy.md` …". `SKILL.md:319` reads "The async-shell/sync-core pattern (agents-arz) is also first-class", and `SKILL.md:330` cites `agents-2b3`. `grep -rnoE "\b(agents|sa)-[a-z0-9]{3}\b" plugins/` finds exactly these two ids, in both the Claude and Codex copies of shatter-advise.
- `docs/specs/2026-06-16-shatter-tractability-taxonomy.md` exists in the repo (21.9 KB).
- `plugins/claude/shatter/skills/shatter-advise/` contains `SKILL.md`, `metadata.json` and `scripts/` (`discover_hotspots.py`, `test_discover_hotspots.py`). `plugins/claude/shatter/skills/shatter-gaps/` contains `SKILL.md` and `metadata.json`. The Codex payload mirrors this.
- `scripts/build-plugins` around line 83 has `_is_ignored(p) = "__pycache__" in parts or suffix == ".pyc"`.
- `scripts/build-plugins` already supports a per-skill `references/` directory: `load_skill` collects it (`references=_collect_companion_files(skill_dir, "references")`, line 123) and `write_skill` copies `references/` and `scripts/` into each payload (lines 177-184). No skill uses it yet. The missing work is supplying the taxonomy there, not adding the mechanism.
- `catalog/skills/shatter-gaps/SKILL.md:41` already references a sibling skill's file, `python3 <skill-dir>/../shatter-advise/scripts/discover_hotspots.py`, so cross-skill references inside the plugin payload are an existing, valid pattern.

## Acceptance criteria

- [ ] The taxonomy ships as a companion reference, for example `references/taxonomy.md`, under shatter-advise, with shatter-gaps citing it by a relative path that resolves in the built payload (or both skills carry or share one copy). The source spec and the shipped copy cannot drift: either the build copies it, or a test compares them.
- [ ] No tracker ids (`agents-*` or `sa-*`) appear in shipped skill text. `grep -rnE "\b(agents|sa)-[a-z0-9]{3}\b" plugins/` returns nothing.
- [ ] `scripts/build-plugins` excludes `test_*.py` (and `*_test.py`) from payloads. `tests/test_build_plugins.py` asserts this, and `find plugins -name 'test_*.py'` is empty.
- [ ] A smoke test reads each built SKILL.md under `plugins/claude/shatter/skills/` and `plugins/codex/shatter/skills/` and checks **bundled-resource references only**: paths that start with `references/`, `scripts/`, `<skill-dir>/` or `../<other-skill>/` (after substituting `<skill-dir>` with the skill's payload directory). Each must resolve to an existing file inside that plugin's payload root (`plugins/<runtime>/shatter/`), so the existing `../shatter-advise/scripts/discover_hotspots.py` reference passes. Paths in the downstream project (`.shatter/...`, `shatter-review/...`, `src/...`), output paths and `<placeholder>` paths are not checked. The test also asserts that no shipped SKILL.md mentions `docs/specs/`. It fails before the change (shatter-advise cites `docs/specs/...`).
- [ ] `scripts/check-plugins-clean` and `python -m pytest tests/` pass (output pasted in the close comment). The patch version bump comes from `scripts/build-plugins` automatically; do not hand-edit it.

## Suggested approach

Use the existing `references/` companion mechanism. Either have build-plugins copy `docs/specs/2026-06-16-shatter-tractability-taxonomy.md` into `shatter-advise/references/taxonomy.md` at build time, or check in the copy and add a test that it equals the spec. shatter-gaps cites it as `../shatter-advise/references/taxonomy.md`, the same pattern it already uses for `discover_hotspots.py`. Replace "(agents-arz)" with a plain description of the pattern.

## Out of scope

- Changing the taxonomy's content.
- sa-d1b bookkeeping. That is close-agents-mirror-issues.

## Dependencies

- None within this tracker. Related: sa-d1b, sa-3lu, sa-arz.

## Priority / Type / Labels

P2 · bug · skills, packaging, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-14. Revised after the Codex cross-check (findings 6, 9).
