---
slug: withdraw-shatter-diff-skill
kind: new
title: "Withdraw the shatter-diff skill: it documents a nonexistent `shatter diff <base-ref> --staged` command, and its pre-commit hook recipe blocks every commit"
priority: P1
type: bug
labels: [skills, cli-contract, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Withdraw the shatter-diff skill: it documents a nonexistent `shatter diff <base-ref> --staged` command, and its pre-commit hook recipe blocks every commit

## Problem

The `shatter-diff` skill ships in plugin 0.1.12 for both Claude and Codex. It tells agents and users to run a diff-scoped exploration command, `shatter diff [<base-ref>] [--staged] [--include-tests] [--output-dir] [--format json|text] [--jobs]`. The shatter CLI has never had that command. The skill also gives a pre-commit hook recipe that aborts the commit when `shatter diff --staged ...` fails. The command always fails with a usage error (exit 2), so anyone who installs the hook cannot commit.

The shatter maintainer made a decision on 2026-09-23. The existing `shatter diff` command, which compares two snapshot files, is being **retired**, and `shatter spec-diff` is the supported regression tool. This work is tracked in the shatter repo as `retire-snapshot-diff`. Today, then, there is:

- no diff-scoped exploration command, under any name;
- a `shatter diff` command that means something different from what the skill says, and that is being removed;
- `shatter spec-diff <OLD> <NEW>`, which compares two spec JSON files (produced by `shatter explore --spec-json`) and exits nonzero on behavioural regressions.

Diff-scoped exploration is future work in shatter epic **str-81xiw** (open). The skill carries no unreleased, experimental or requires marker, so nothing stops it from shipping.

This issue corrects the plugin to match what exists today. It does **not** pick the name of the future command. When `shatter diff` is retired, the `diff` name becomes free, and str-81xiw decides whether diff-scoped exploration is called `diff`, `diff-explore` or something else.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807` and the shatter binary built from audit HEAD (`target/debug/shatter`).

- `catalog/skills/shatter-diff/SKILL.md:46` gives the synopsis `shatter diff [<base-ref>] [--staged] [--include-tests]`. Lines 52-66 describe base-ref defaults and `--staged` precedence.
- `catalog/skills/shatter-diff/SKILL.md:122-136` is the pre-commit hook recipe:
  ```
  if ! shatter diff --staged --format json --output-dir "$out"; then
    echo "shatter diff failed to run; commit aborted." >&2
  ```
- `catalog/plugins.json:5` lists `shatter-diff` in the `shatter` plugin's `skills` array. The built copies are `plugins/claude/shatter/skills/shatter-diff/` and `plugins/codex/shatter/skills/shatter-diff/`. `.claude-plugin/marketplace.json` publishes the plugin as version `0.1.12`.
- `catalog/skills/shatter-diff/metadata.json` is `{"recommended_model": "low", "audience": ["user"]}`, with no status field.
- The real CLI (shatter `shatter-cli/src/args.rs:1432`, `Diff { snapshot, current, json }`):
  ```
  $ shatter diff --staged
  error: unexpected argument '--staged' found
  Usage: shatter diff [OPTIONS] <SNAPSHOT> <CURRENT>        (exit 2)
  $ shatter diff-explore
  error: unrecognized subcommand 'diff-explore'
  $ shatter spec-diff --help
  Usage: shatter spec-diff [OPTIONS] <OLD> <NEW>
  ```
- Tracker: sa-tyb ("shatter diff: first-class diff-scoped exploration command") is CLOSED with the reason "a850d93b… landed on main". Only the skill text landed. Shatter str-81xiw (epic "Diff-scoped exploration") and str-81xiw.2 are OPEN.

## Acceptance criteria

- [ ] `shatter-diff` no longer appears in any published payload. `plugins/claude/shatter/skills/shatter-diff/` and `plugins/codex/shatter/skills/shatter-diff/` are absent after `scripts/build-plugins`. This happens either because the skill is removed from `catalog/plugins.json`, or because a status marker (for example `metadata.json` `"status": "unreleased"`, with `requires` naming shatter str-81xiw) is honoured by `build-plugins`. If a marker is used, a test in `tests/` shows that a skill carrying it is excluded from both payloads.
- [ ] No catalog or published file contains a `shatter diff --staged` pre-commit hook recipe. `grep -rn "shatter diff" catalog plugins README.md` returns no instruction to run `shatter diff` with a base ref or `--staged`.
- [ ] Where the plugin talks about regression checking (run-shatter, report-shatter-issues, interpret-shatter-spec, or README, whichever mentions it), it points to `shatter spec-diff <OLD> <NEW>` on spec JSON from `shatter explore --spec-json`. Each flag it cites appears in `shatter spec-diff --help` of the current build.
- [ ] No plugin text names the future diff-scoped command (`diff` or `diff-explore`). A withdrawn or unreleased skill may say only that diff-scoped exploration is planned in shatter str-81xiw.
- [ ] The plugin version in `.claude-plugin/marketplace.json` (and the Codex manifest) is bumped so installed users receive the withdrawal. `scripts/check-plugins-clean` and `python -m pytest tests/` pass. Paste the output of both in the close comment.
- [ ] sa-tyb carries the reopen-note that points here (sa-tyb-reopen-note).

## Suggested approach

1. Remove `shatter-diff` from the `skills` array in `catalog/plugins.json`, and either keep `catalog/skills/shatter-diff/` with an unreleased marker or delete it. Deleting is simpler. Keeping it with a marker is only worthwhile if cli-contract-test adds the marker mechanism at the same time.
2. Run `scripts/build-plugins` and confirm that both payload directories are gone.
3. Add a short "Checking for regressions" paragraph where the plugin already discusses re-runs (run-shatter is the likeliest place): explore with `--spec-json` before and after, then run `shatter spec-diff old.json new.json`. Nonzero exit means a regression; inconclusive-only results exit 0.
4. Bump the plugin version.

## Out of scope

- Implementing diff-scoped exploration. That is shatter epic str-81xiw.
- Choosing that command's name. That is for str-81xiw, now that retire-snapshot-diff frees `diff`.
- Retiring the snapshot `shatter diff` in the engine. That is shatter `retire-snapshot-diff`.
- The generic skill-vs-CLI contract test. That is cli-contract-test.

## Dependencies

- None within this tracker. The fix does not have to wait for shatter.
- Cross-repo context (not a bd dependency): shatter `retire-snapshot-diff` and epic str-81xiw. A rewritten diff skill is a new issue, filed once str-81xiw ships a command.
- Related: cli-contract-test (it would have caught this).

## Priority / Type / Labels

P1 · bug · skills, cli-contract, audit-2026-09-22

## Source

Shatter audit 2026-09-22, findings plugins-01, goals-11 and artifacts-11. Maintainer decision D2 (2026-09-23).
