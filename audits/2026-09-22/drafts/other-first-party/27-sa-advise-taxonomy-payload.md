# shatter-advise/shatter-gaps cite a taxonomy spec that is not shipped; stale agents-arz ID; test file in payload

## Filing metadata

- tracker/repo: shatter-agents
- action: create new issue
- type: bug
- priority: P2
- labels: audit-2026-09-22
- parent: repo epic (see INDEX)
- dedupe relation: new (related sa-d1b, sa-3lu, sa-arz)
- source findings: plugins-14

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

`catalog/skills/shatter-advise/SKILL.md` (around lines 317-319) says: "The taxonomy spec at docs/specs/2026-06-16-shatter-tractability-taxonomy.md (in the shatter-agents repo) defines all named patterns". Findings cite those patterns by `pattern_id`. The published payload `plugins/claude/shatter/skills/shatter-advise/` contains only SKILL.md, metadata.json and scripts/, and shatter-gaps only SKILL.md and metadata.json. Installed agents therefore cannot read the catalog. The skill also cites `agents-arz`, but the tracker issue is `sa-arz` (deferred). `scripts/test_discover_hotspots.py` ships in the payload.

## Acceptance criteria

- [ ] The taxonomy ships as `references/taxonomy.md` under shatter-advise and shatter-gaps (or one shared reference both cite by relative path).
- [ ] Tracker IDs in the skill text are replaced with prose.
- [ ] `scripts/build-plugins` excludes `test_*.py` from payloads, and `check-plugins-clean` passes.
- [ ] A smoke check reads the skill from the built payload directory and resolves every referenced file.

## Source

Shatter audit 2026-09-22 finding plugins-14.
