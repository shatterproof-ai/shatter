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

The `shatter-diff` skill ships in plugin 0.1.12 for both Claude and Codex. It tells agents and users to run a diff-scoped exploration command, `shatter diff [<base-ref>] [--staged] [--include-tests] [--output-dir] [--format json|text] [--jobs]`. The shatter CLI has never had that command. The skill also gives a pre-commit hook recipe that aborts the commit when `shatter diff --staged ...` fails. The command always fails with a usage error (exit 2), so anyone who installed the hook cannot commit. Removing the skill from the payload does **not** repair a hook that a user already copied into `.git/hooks/pre-commit` or a hook manager, so the withdrawal must also tell affected users how to recover.

The shatter maintainer made a decision on 2026-09-23. The existing `shatter diff` command, which compares two snapshot files, is being **retired**, and `shatter spec-diff` is the supported regression tool. That engine work is the shatter-repo issue **<retire-snapshot-diff id>** (audit slug `retire-snapshot-diff`, bucket shatter-artifacts-correctness; the filer substitutes the id). Today, then, there is:

- no diff-scoped exploration command, under any name;
- a `shatter diff` command that means something different from what the skill says, and that is being removed;
- `shatter spec-diff <OLD> <NEW>`, which compares two spec JSON files (produced by `shatter explore --spec-json`) and exits nonzero on behavioural regressions.

Diff-scoped exploration is future work in shatter epic **str-81xiw** (open). The skill carries no unreleased, experimental or requires marker, so nothing stops it from shipping.

This issue corrects the plugin to match what exists today. It does **not** pick the name of the future command. When `shatter diff` is retired, the `diff` name becomes free, and str-81xiw decides whether diff-scoped exploration is called `diff`, `diff-explore` or something else.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807` and a shatter CLI built from shatter commit `70465921` (the audit worktree base; `shatter-cli/src/args.rs` is identical at `16794cef`). Reproduce with `git -C <shatter> checkout 70465921 && cargo build -p shatter-cli`.

- `catalog/skills/shatter-diff/SKILL.md:46` gives the synopsis `shatter diff [<base-ref>] [--staged] [--include-tests]`. Lines 52-66 describe base-ref defaults and `--staged` precedence.
- `catalog/skills/shatter-diff/SKILL.md:120-142` is the "Pre-commit hook example". It says to save the script as `.git/hooks/pre-commit` "(or add to an existing hook manager)", and the script contains:
  ```
  out="$(mktemp -d)"
  if ! shatter diff --staged --format json --output-dir "$out"; then
    echo "shatter diff failed to run; commit aborted." >&2
    ...
    exit 1
  fi
  ```
- `catalog/plugins.json:5` lists `shatter-diff` in the `shatter` plugin's `skills` array. The built copies are `plugins/claude/shatter/skills/shatter-diff/` and `plugins/codex/shatter/skills/shatter-diff/`. `.claude-plugin/marketplace.json:17` publishes the plugin as version `0.1.12`.
- `catalog/skills/shatter-diff/metadata.json` is `{"recommended_model": "low", "audience": ["user"]}`, with no status field.
- The real CLI (shatter `shatter-cli/src/args.rs:1432`, `Diff { snapshot, current, json }`; `:1455` `SpecDiff { old, new, json }`):
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

- [ ] `catalog/skills/shatter-diff/` is deleted and `shatter-diff` is removed from `catalog/plugins.json`. (Deletion, not an unreleased marker: the design text survives in git history and sa-tyb, and a future skill will be rewritten against whatever str-81xiw ships.) After `scripts/build-plugins`, `find plugins -path '*shatter-diff*'` prints nothing.
- [ ] `grep -rnE "shatter diff( |$)" catalog plugins README.md INSTALL.md` prints nothing, except lines inside the recovery note below.
- [ ] **Recovery for users who installed the hook.** README.md gains a short "Removed skills" section (and the same text goes in the close comment and the plugin release note, if any) that: names shatter-diff as withdrawn; says any hook containing `shatter diff --staged` blocks every commit; and gives the exact removal: delete the block from the `out="$(mktemp -d)"` line through the matching `fi` (and the trailing `echo "Shatter explored ..."` line), or delete `.git/hooks/pre-commit` if it contains nothing else.
- [ ] **Detection for users who installed the hook.** `shatter-doctor` checks `.git/hooks/pre-commit` (and, if present, `.pre-commit-config.yaml` / `.husky/pre-commit` / `lefthook.yml`) for the string `shatter diff --staged` and, when found, reports `stale shatter-diff hook: blocks every commit` with the removal text. Implemented as a stdlib-only helper under `catalog/skills/shatter-doctor/scripts/` so it is testable.
- [ ] **Recovery is proven by a test**, `tests/test_shatter_diff_hook_recovery.py`, which: creates a temp git repo; writes a `pre-commit` hook consisting of an unrelated line that appends to a marker file plus the stanza copied verbatim from `119b807:catalog/skills/shatter-diff/SKILL.md:127-142` (checked in as a fixture); puts a stub `shatter` on `PATH` that exits 2 (as the real CLI does); asserts `git commit` fails; applies the documented removal; asserts `git commit` succeeds **and** the marker file was written (unrelated hook logic still runs). The same test asserts the shatter-doctor helper flags the pre-removal hook and is silent on the post-removal hook.
- [ ] Where the plugin talks about regression checking (run-shatter, report-shatter-issues or interpret-shatter-spec, whichever mentions re-runs), it points to `shatter spec-diff <OLD> <NEW>` on spec JSON from `shatter explore --spec-json`. Each flag it cites appears in `shatter spec-diff --help` of a build at or after shatter `70465921`.
- [ ] No plugin text names the future diff-scoped command (`diff` or `diff-explore`). Text may say only that diff-scoped exploration is planned in shatter str-81xiw.
- [ ] The plugin patch version is bumped by `scripts/build-plugins` (automatic per AGENTS.md "Patch versions auto-bump"; do not hand-edit it). `.claude-plugin/marketplace.json` and the Codex manifest show the new version. `scripts/check-plugins-clean` and `python -m pytest tests/` pass. Paste the output of both, plus the recovery test run, in the close comment.
- [ ] sa-tyb carries the reopen-note that points here (sa-tyb-reopen-note).

## Suggested approach

1. Delete `catalog/skills/shatter-diff/` and its `catalog/plugins.json` entry; run `scripts/build-plugins`.
2. Write the recovery note and the shatter-doctor hook check, then the recovery test.
3. Add a short "Checking for regressions" paragraph where the plugin already discusses re-runs (run-shatter is the likeliest place): explore with `--spec-json` before and after, then run `shatter spec-diff old.json new.json`. Nonzero exit means a regression; inconclusive-only results exit 0.

## Out of scope

- Implementing diff-scoped exploration. That is shatter epic str-81xiw.
- Choosing that command's name. That is for str-81xiw, now that <retire-snapshot-diff id> frees `diff`.
- Retiring the snapshot `shatter diff` in the engine. That is shatter <retire-snapshot-diff id>.
- The generic skill-vs-CLI syntax test. That is cli-contract-test.

## Dependencies

- None within this tracker. The fix does not have to wait for shatter.
- Cross-repo context (not a bd dependency): shatter <retire-snapshot-diff id> and epic str-81xiw. A rewritten diff skill is a new issue, filed once str-81xiw ships a command.
- Related: cli-contract-test (it would have caught the syntax half of this).

## Priority / Type / Labels

P1 · bug · skills, cli-contract, audit-2026-09-22

## Source

Shatter audit 2026-09-22, findings plugins-01, goals-11 and artifacts-11. Maintainer decision D2 (2026-09-23). Revised after the Codex cross-check (findings 7, 10).
