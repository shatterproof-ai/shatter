# shatter-diff skill documents a nonexistent `shatter diff <base-ref> --staged` command; its pre-commit hook blocks every commit

## Filing metadata

- tracker/repo: shatter-agents
- action: create new issue
- type: bug
- priority: P1
- labels: audit-2026-09-22
- parent: repo epic (see INDEX)
- dedupe relation: duplicate-closed-but-unfixed (sa-tyb) -> new issue referencing sa-tyb
- source findings: plugins-01, goals-11, artifacts-11

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

`catalog/skills/shatter-diff/SKILL.md` (released in plugin 0.1.12) documents `shatter diff [<base-ref>] [--staged] [--include-tests] [--output-dir] [--format json|text] [--jobs]` at around line 46. Around line 133 it gives a pre-commit hook recipe:

```
if ! shatter diff --staged --format json --output-dir "$out"; then echo 'commit aborted'; exit 1
```

The real CLI has no such command. `shatter diff` is snapshot comparison: `shatter diff [OPTIONS] <SNAPSHOT> <CURRENT>` (shatter `shatter-cli/src/args.rs` around 1429-1437).

- `shatter diff --staged ...` -> `error: unexpected argument '--staged' found`, exit 2.
- `shatter diff-explore` -> unrecognized subcommand.

Anyone who installs the hook cannot commit.

sa-tyb ("shatter diff: first-class diff-scoped exploration command") was closed as "a850d93 landed on main" when only the skill text landed. The engine work moved to the shatter tracker as epic **str-81xiw** (renamed `diff-explore`; str-81xiw.2 "diff-explore CLI shell" is open). The skill carries no unreleased or requires marker.

## Acceptance criteria

- [ ] `shatter-diff` is removed from the published payload (`plugins/claude/shatter/skills/`), or excluded via a status/requires marker, until str-81xiw.2 and .4 land. `scripts/check-plugins-clean` passes.
- [ ] When the engine lands, the skill is renamed or rewritten against the real `diff-explore` surface and checks `shatter diff-explore --help` before use.
- [ ] The pre-commit hook recipe is removed or rewritten so it cannot block commits on an unsupported CLI.
- [ ] sa-tyb carries a note pointing to this issue.

## Out of scope

Implementing diff-explore, which is the shatter repo's str-81xiw.

## Dependencies

Blocked by shatter str-81xiw (cross-repo, not expressible as a bd dep).

## Source

Shatter audit 2026-09-22 findings plugins-01, goals-11 and artifacts-11.
