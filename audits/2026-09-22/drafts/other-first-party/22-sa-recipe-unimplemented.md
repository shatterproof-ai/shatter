# compose-shatter-recipe and run-shatter document recipes and a stubs: registry that neither the engine nor run_targets.py implements

## Filing metadata

- tracker/repo: shatter-agents
- action: create new issue
- type: bug
- priority: P1
- labels: audit-2026-09-22
- parent: repo epic (see INDEX)
- dedupe relation: duplicate-closed-but-unfixed (sa-yyt) -> new issue referencing sa-yyt
- source findings: plugins-02

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

The skills describe behaviour that does not exist:

- `catalog/skills/compose-shatter-recipe/SKILL.md` documents `.shatter/recipes/<target-id>/<recipe-name>.json` (around line 145), a `stubs:` section in `.shatter/config.yaml` (implements/lang/source/factory; around lines 191-239), and resolver errors such as `unsupported recipe schemaVersion <n>` (around line 342).
- `catalog/skills/run-shatter/SKILL.md` (around lines 69-89) says it runs each target once per recipe and validates recipes first.

## Current code facts

- `grep -i recipe catalog/skills/run-shatter/scripts/*.py` -> 0 matches (run_targets.py, about 360 lines).
- In the shatter engine (`shatter-core/src`, `shatter-cli/src`, `shatter-rust/src`), "recipe" matches only unrelated reconstruction fields. No config struct has a `stubs` key, and there is no `.shatter/recipes` loader.
- sa-yyt ("implement: recipe schema and stub-registration…") was closed with "compose-shatter-recipe skill landed on main (3b7aec4)". sa-mty (the design) is legitimately closed.

## Acceptance criteria

- [ ] The recipe sections of both skills are marked as a design proposal that the engine does not yet execute, or removed. run-shatter no longer claims per-recipe runs.
- [ ] If per-recipe runs are kept, `run_targets.py` implements them with a test.
- [ ] A shatter-repo issue exists for the engine side (recipe resolver, `stubs` config, frontend stub construction) and is linked here in the body.
- [ ] sa-yyt carries a note pointing to this issue.

## Source

Shatter audit 2026-09-22 finding plugins-02.
