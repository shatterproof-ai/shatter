---
slug: sa-yyt-reopen-note
kind: reopen-note
title: "Comment on closed sa-yyt: recipe discovery and the stubs registry are documented but not implemented"
priority: P1
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: []
existing_id: sa-yyt
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Comment on closed sa-yyt

Target: **sa-yyt** ("implement: recipe schema and stub-registration for independent per-parameter resource stubbing"), CLOSED with the reason "compose-shatter-recipe skill landed on main (3b7aec4)". Post the comment below. Do not reopen: the follow-up work is tracked in the new issue.

## Comment text

Audit 2026-09-22 (finding plugins-02): this issue was closed when the compose-shatter-recipe skill text landed, but nothing implements what the skill describes.

- `catalog/skills/compose-shatter-recipe/SKILL.md:145` documents `.shatter/recipes/<target-id>/<name>.json`, lines 221-239 document a `stubs:` section in `.shatter/config.yaml`, and line 342 documents resolver errors such as `unsupported recipe schemaVersion <n>`. The shatter engine has no recipe loader, no `stubs` config key and no such errors.
- `catalog/skills/run-shatter/SKILL.md:69-89` says run-shatter discovers recipes and runs each target once per recipe. `run_targets.py` contains no occurrence of "recipe".

Follow-up: **<recipes-marked-design-only id>** marks the recipe material as a design proposal (or withdraws it), removes the per-recipe-run claim from run-shatter, and links a shatter-repo issue for the engine side. sa-mty (the design) stays closed.
