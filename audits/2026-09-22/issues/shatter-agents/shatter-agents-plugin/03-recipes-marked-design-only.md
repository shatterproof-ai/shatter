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

This issue has **one** completion path: withdraw the recipe material from the published payload and remove the per-recipe-run claim. Implementing recipes is engine work first (there is nothing for `run_targets.py` to call), and it is tracked separately in the shatter repo.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807` and shatter source at commit `70465921` (the audit worktree base).

- `catalog/skills/compose-shatter-recipe/SKILL.md:145` documents `.shatter/recipes/<target-id>/<recipe-name>.json`. Lines 221-239 document the `stubs:` section of `.shatter/config.yaml` (implements, lang, source, factory). Line 342 documents the error `unsupported recipe schemaVersion <n>`. Line 501 is the "Write the recipe" step.
- `catalog/skills/run-shatter/SKILL.md:69-89` is the section "Recipe discovery and runs", which enumerates `.shatter/recipes/<target-id>/*.json` and runs the target "once per recipe".
- `grep -ci recipe catalog/skills/run-shatter/scripts/run_targets.py` returns 0 (the file is 360 lines).
- Shatter engine at `70465921` (`shatter-core/src`, `shatter-cli/src`, `shatter-rust/src`): "recipe" matches only unrelated reconstruction-recipe fields. No config struct has a `stubs` key, and nothing loads `.shatter/recipes`.
- `catalog/plugins.json:5` ships `compose-shatter-recipe` in the `shatter` plugin. It is present under `plugins/claude/shatter/skills/` and `plugins/codex/shatter/skills/`.
- sa-yyt is CLOSED with the reason "compose-shatter-recipe skill landed on main (3b7aec4)".

## Acceptance criteria

- [ ] `compose-shatter-recipe` is removed from `catalog/plugins.json`. After `scripts/build-plugins`, `find plugins -path '*compose-shatter-recipe*'` prints nothing. The catalog source may stay in `catalog/skills/compose-shatter-recipe/` as design material, but its SKILL.md then opens with "Design proposal, not shipped: the Shatter engine does not read recipes or `stubs:` (see <engine issue id>)".
- [ ] run-shatter's "Recipe discovery and runs" section is deleted, not relabelled. `grep -rniE "recipe|stubs:" plugins/claude/shatter/skills/run-shatter plugins/codex/shatter/skills/run-shatter` prints nothing.
- [ ] No published skill links to or names compose-shatter-recipe: `grep -rn "compose-shatter-recipe" plugins README.md INSTALL.md` prints nothing.
- [ ] `run_targets.py` is unchanged by this issue (no per-recipe code is added).
- [ ] The engine-side work (recipe resolver, `stubs` config, frontend stub construction) is tracked in the shatter repo. Before closing, run `bd search recipe` in `/home/ketan/project/shatter`; link the existing issue if there is one, otherwise file one citing sa-mty's design. Its id replaces `<engine issue id>` above and is linked in this issue's body.
- [ ] `scripts/build-plugins` (which auto-bumps the patch version per AGENTS.md), `scripts/check-plugins-clean` and `python -m pytest tests/` pass. Paste the output and the two `find`/`grep` results above in the close comment.
- [ ] sa-yyt carries the reopen-note that points here (sa-yyt-reopen-note).

## Suggested approach

Drop the plugins.json entry and rebuild. Put the design-proposal banner on the catalog copy so the schema text is kept for the engine work. Delete the run-shatter section outright.

## Out of scope

- Implementing the recipe resolver or the `stubs` config in the engine, or per-recipe runs in `run_targets.py`. Both follow the shatter-repo engine issue; re-shipping compose-shatter-recipe is a new issue filed after the engine reads recipes.
- The general skill-vs-CLI syntax test. That is cli-contract-test.

## Dependencies

- None within this tracker.
- Related: withdraw-shatter-diff-skill (same pattern), cli-contract-test (scans published payloads only, so the unshipped catalog copy does not trip it).

## Priority / Type / Labels

P1 · bug · skills, cli-contract, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-02. Revised after the Codex cross-check (findings 3, 5, 10).
