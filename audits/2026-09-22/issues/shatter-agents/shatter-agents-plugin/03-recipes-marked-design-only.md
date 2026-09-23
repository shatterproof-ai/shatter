---
slug: recipes-marked-design-only
kind: new
title: "compose-shatter-recipe and run-shatter document recipes and a stubs: registry that neither the engine nor run_targets.py implements"
priority: P1
type: bug
labels: [skills, cli-contract, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# compose-shatter-recipe and run-shatter document recipes and a stubs: registry that neither the engine nor run_targets.py implements

## Problem

Two published skills describe behaviour that does not exist anywhere:

- `compose-shatter-recipe` tells users to write recipe files under `.shatter/recipes/<target-id>/<recipe-name>.json` and add a `stubs:` section to `.shatter/config.yaml`. It also lists resolver errors that Shatter supposedly raises. The shatter engine has no recipe loader, no `stubs` config key and no such errors.
- `run-shatter` says it discovers each target's recipes, validates them and runs the target once per recipe. Its script `run_targets.py` does none of that.

Users who follow compose-shatter-recipe write files that nothing reads, and run-shatter reports runs it never performed. sa-yyt ("implement: recipe schema and stub-registration…") was closed when the skill text landed. sa-mty (the design) is legitimately closed.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807` and shatter audit HEAD.

- `catalog/skills/compose-shatter-recipe/SKILL.md:145` documents `.shatter/recipes/<target-id>/<recipe-name>.json`. Lines 221-239 document the `stubs:` section of `.shatter/config.yaml` (implements, lang, source, factory). Line 342 documents the error `unsupported recipe schemaVersion <n>`. Line 501 is the "Write the recipe" step.
- `catalog/skills/run-shatter/SKILL.md:69-89` is the section "Recipe discovery and runs", which enumerates `.shatter/recipes/<target-id>/*.json` and runs the target "once per recipe".
- `grep -ci recipe catalog/skills/run-shatter/scripts/*.py` returns 0 for `run_targets.py` (360 lines).
- Shatter engine (`shatter-core/src`, `shatter-cli/src`, `shatter-rust/src`): "recipe" matches only unrelated reconstruction-recipe fields. No config struct has a `stubs` key, and nothing loads `.shatter/recipes`.
- `catalog/plugins.json:5` ships `compose-shatter-recipe` in the `shatter` plugin. It is present under `plugins/claude/shatter/skills/` and `plugins/codex/shatter/skills/`.
- sa-yyt is CLOSED with the reason "compose-shatter-recipe skill landed on main (3b7aec4)".

## Acceptance criteria

- [ ] The recipe sections of both skills are either removed or marked at the top of each section as "Design proposal: the Shatter engine does not yet read recipes or `stubs:`". Neither skill instructs an agent to write `.shatter/recipes/` or `stubs:` as if Shatter will use them. If compose-shatter-recipe then has no executable purpose, it is withdrawn from `catalog/plugins.json` or marked unreleased (as in withdraw-shatter-diff-skill).
- [ ] run-shatter no longer claims per-recipe runs. `grep -n -i "once per recipe\|recipe discovery" catalog/skills/run-shatter/SKILL.md` returns nothing, or the matches sit inside a clearly marked design-proposal note.
- [ ] If per-recipe runs are kept as a real feature instead, `run_targets.py` implements them, and a test in `tests/test_run_targets.py` fails before the change and passes after it.
- [ ] A shatter-repo issue exists for the engine side (recipe resolver, `stubs` config and frontend stub construction), and its id is linked in this issue's body before close.
- [ ] `scripts/build-plugins`, `scripts/check-plugins-clean` and `python -m pytest tests/` pass (output pasted in the close comment), and the plugin version is bumped.
- [ ] sa-yyt carries the reopen-note that points here (sa-yyt-reopen-note).

## Suggested approach

Mark the recipe material as a design proposal, keeping the schema text for the future engine work, and withdraw compose-shatter-recipe from the payload until the engine reads recipes. Remove the recipe section from run-shatter entirely rather than adding code to a script that has no engine support behind it. File the shatter engine issue, citing sa-mty's design.

## Out of scope

- Implementing the recipe resolver or the `stubs` config in the engine. That belongs to the shatter tracker.
- The general contract test between skills and the CLI. That is cli-contract-test.

## Dependencies

- None within this tracker.
- Related: withdraw-shatter-diff-skill (same pattern), cli-contract-test.

## Priority / Type / Labels

P1 · bug · skills, cli-contract, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-02.
