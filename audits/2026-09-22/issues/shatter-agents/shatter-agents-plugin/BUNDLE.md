# Bundle: shatter-agents-plugin

- **Bucket:** shatter-agents-plugin. Plugin skills that document CLI behaviour the engine does not have, plus contract tests, delegation, CI wiring and payload hygiene.
- **Repo / tracker:** shatter-agents. bd in /home/ketan/project/shatter-agents, prefix `sa` (the config has no issue-prefix; the prefix is taken from existing ids such as sa-tyb).
- **Parent epic:** "Epic: Audit 2026-09-22 findings (shatter-agents plugin)".
- **Contents:** 13 entries. 9 are new issues (2 at P1, 6 at P2, 1 at P3), 2 are reopen-notes on closed issues (sa-tyb, sa-yyt) and 2 are notes on existing issues (sa-oio, sa-d8j). Nothing has been filed. Revised 2026-09-23 after the Codex cross-check; see REVISION.md.
- **Verification basis:** shatter-agents `119b807` and a shatter CLI built from shatter commit `70465921` (the audit worktree base; `shatter-cli/src/args.rs` is identical at `16794cef`), re-checked on 2026-09-23. Each draft names these SHAs in its Evidence section.

## Maintainer decisions (2026-09-23)

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix. Fix them (the Z3 header or static link on Windows; openssl-sys under cross for aarch64) rather than dropping them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command (`shatter diff`) and the unused Snapshot writer path. spec-diff is THE regression tool, and SPEC, README and QUICKSTART are updated to match. Whether diff-scoped exploration (str-81xiw) later takes the `diff` name is left to str-81xiw. The plugin's `shatter diff --staged` docs must be corrected to what exists today. *(Applied in 01 and 02.)*
- **D3 Concolic positioning:** measure first. A P1 default-vs-concolic benchmark, a P1 fix for concolic early termination, and then a follow-up decision issue. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import in shatter and move tracker sync to a Dolt remote. AGENTS.md drops `bd sync`, str-qwua7.28 is superseded, and bento beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT fix and no hook-bypass guidance. *(Touched only by 11, which tells implementers to follow the current sync convention and not to hand-edit the JSONL.)*
- **D5 Git identity:** the leaked `[user]` section was already removed. Add a .mailmap, a git-state drift check and a fixture `.git/config` snapshot test. *(Not applicable to this bucket.)*
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. Nothing is filed by agents.

## Cross-repo references (not bd dependencies)

- Shatter-repo draft slugs are written as `<slug id>` placeholders inside draft text; the filer replaces them with the filed ids. `<retire-snapshot-diff id>` (bucket shatter-artifacts-correctness) and shatter epic str-81xiw are context for 01, 02 and 05. `<release-publish-and-install-smoke id>` (bucket shatter-ci-workflows) is context for 05 and 08: shatter has no published release today, so pinned `BUILD=` downloads do not resolve.
- In reopen-note and note text, `<slug id>` placeholders for this bucket's slugs are likewise replaced by the filer.
- Engine gaps found while revising (not filed here; for the shatter tracker): `shatter list-targets` selects sources under `.shatter/cache/harness/` (13), and no shatter command rejects a malformed `.shatter/config.yaml` (06).

## Index

| # | Slug | Kind | P | Title |
|---|---|---|---|---|
| 01 | withdraw-shatter-diff-skill | new | P1 | Withdraw the shatter-diff skill: it documents a nonexistent `shatter diff <base-ref> --staged` command, and its pre-commit hook recipe blocks every commit |
| 02 | sa-tyb-reopen-note | reopen-note (sa-tyb) | P1 | Comment on closed sa-tyb: only the skill text landed; the documented command does not exist |
| 03 | recipes-marked-design-only | new | P1 | compose-shatter-recipe and run-shatter document recipes and a stubs: registry that neither the engine nor run_targets.py implements |
| 04 | sa-yyt-reopen-note | reopen-note (sa-yyt) | P1 | Comment on closed sa-yyt: recipe discovery and the stubs registry are documented but not implemented |
| 05 | cli-contract-test | new | P2 | Add a CLI-syntax contract test between published skills and a pinned shatter build |
| 06 | delegate-discovery-to-engine | new | P2 | shatter-doctor should report the engine's own `shatter doctor` and move its PyYAML-dependent config check into a tested helper |
| 07 | sa-oio-wrapper-convention | note-to-existing (sa-oio) | P2 | Note on sa-oio: reconcile the wrapper convention with the default-deny execution policy; validate the subcommand in run-shatter |
| 08 | wire-shatter-ci-standalone | new | P2 | wire-shatter-ci: template hard-codes plugin-internal run_targets.py against its own fallback guidance, Verify cannot catch it, and install.sh is fetched from main |
| 09 | advise-taxonomy-payload | new | P2 | shatter-advise cites an unshipped taxonomy spec and shatter-gaps references the taxonomy with no resolvable source; stale agents-arz id; test file shipped in the payload |
| 10 | claude-md-imports-agents-md | new | P2 | shatter-agents CLAUDE.md points at AGENTS.md without importing it |
| 11 | close-agents-mirror-issues | new | P2 | Close agents-* mirror duplicates of sa-* issues and stale-open shipped work (sa-d1b) |
| 12 | skill-status-metadata | new | P3 | Skill metadata.json status/requires_shatter fields that build-plugins honours, so unreleased skills can live in the catalog without shipping |
| 13 | sa-d8j-engine-discovery-note | note-to-existing (sa-d8j) | P2 | Note on sa-d8j: `shatter list-targets` is not a drop-in replacement for run_targets.py discovery; fix the prune locally |

<!-- begin drafts -->

---

<!-- file: 01-withdraw-shatter-diff-skill.md -->

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

---

<!-- file: 02-sa-tyb-reopen-note.md -->

---
slug: sa-tyb-reopen-note
kind: reopen-note
title: "Comment on closed sa-tyb: only the skill text landed; the documented command does not exist"
priority: P1
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: []
existing_id: sa-tyb
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Comment on closed sa-tyb

Target: **sa-tyb** ("shatter diff: first-class diff-scoped exploration command"), CLOSED with the reason "a850d93b0f2d012143f88e12eaeb021cd8c8c4b5 landed on main". Post the comment below. Do not reopen: the follow-up work is tracked in the new issue.

## Comment text

Audit 2026-09-22 (finding plugins-01): this issue was closed when the `shatter-diff` skill text landed, but the command the skill documents does not exist. `shatter diff --staged` fails with `error: unexpected argument '--staged' found`, and `Usage: shatter diff [OPTIONS] <SNAPSHOT> <CURRENT>` exits 2. `shatter diff-explore` is an unrecognized subcommand. As a result, the skill's pre-commit hook recipe (`catalog/skills/shatter-diff/SKILL.md:120-142` at `119b807`) aborts every commit.

Current state:
- The engine work for diff-scoped exploration is shatter epic **str-81xiw** (open). It has not shipped.
- On 2026-09-23 the shatter maintainer decided to retire the snapshot-comparison `shatter diff` (shatter <retire-snapshot-diff id>). `shatter spec-diff` is the supported regression tool. Once the retirement lands, the `diff` name is free, and str-81xiw decides what the diff-scoped command is called.

Follow-up: **<withdraw-shatter-diff-skill id>** withdraws the skill and its hook recipe from the published payload, documents and tests how users who installed the hook remove it, and points readers to `shatter spec-diff`. A new skill for diff-scoped exploration should be filed once str-81xiw ships a command, and should be closed only after the skill's commands have been run against a real build.

---

<!-- file: 03-recipes-marked-design-only.md -->

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

---

<!-- file: 04-sa-yyt-reopen-note.md -->

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

Follow-up: **<recipes-marked-design-only id>** withdraws compose-shatter-recipe from the published payload (the catalog copy stays as a labelled design proposal), deletes the per-recipe-run section from run-shatter, and links a shatter-repo issue for the engine side. sa-mty (the design) stays closed.

---

<!-- file: 05-cli-contract-test.md -->

---
slug: cli-contract-test
kind: new
title: "Add a CLI-syntax contract test between published skills and a pinned shatter build"
priority: P2
type: task
labels: [testing, cli-contract, ci, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: [withdraw-shatter-diff-skill, recipes-marked-design-only]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Add a CLI-syntax contract test between published skills and a pinned shatter build

## Problem

Nothing checks that the `shatter` subcommands and flags cited in published shatter-agents skills exist in the shatter CLI. shatter-agents CI runs only `scripts/check-plugins-clean` and `python -m pytest tests/`, and no test invokes a shatter binary. The shatter repo does not reference shatter-agents at all, and its own CLI-surface drift check (str-wurp, still open) covers only shatter's SPEC.md and gauntlet. As a result, `shatter diff --staged` (withdraw-shatter-diff-skill) shipped undetected.

**What this test does and does not protect.** It checks CLI *syntax* only: every cited subcommand and long flag exists in the pinned build's `--help`. It cannot tell whether a skill's *described behaviour* happens (for example, that recipes are discovered or a config key is consumed; recipes-marked-design-only would **not** have been caught by it). Behavioural checks stay with the per-skill tests that exercise real behaviour: the wrapper run test proposed on sa-oio, the rendered-workflow test in wire-shatter-ci-standalone, and the hook-recovery test in withdraw-shatter-diff-skill.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807` and shatter `70465921`.

- `.github/workflows/ci.yml` has two jobs: `build-clean` (`scripts/check-plugins-clean`) and `tests` (`python -m pytest tests/ -v`).
- `tests/` has 12 `test_*.py` files plus `fixtures/` (for example `test_skills_load.py` and `test_run_targets.py`). None runs `shatter` or parses `--help`.
- In the shatter repo, `grep -r "shatter-agents\|shatterproof-ai/agents"` (excluding `target/`, `audits/` and `.beads/`) finds nothing. The `cli-surface-drift` check in `docs/DRIFT-PATROL.md` is still pending on str-wurp.
- The shatter project has **no published GitHub release**: `gh release list -R shatterproof-ai/shatter` is empty. That is tracked in the shatter repo as <release-publish-and-install-smoke id> (audit slug `release-publish-and-install-smoke`, bucket shatter-ci-workflows; the filer substitutes the id). A pinned binary therefore has to be built from a pinned shatter commit for now.
- Building `shatter-cli` is not a bare `cargo build`: it links Z3 and embeds the Go, TypeScript and Rust frontends, so a build job needs Z3 headers/libs, a Go toolchain and Node, as shatter's own `.github/workflows/ci.yml` sets up.

## Acceptance criteria

- [ ] **Scope is the published payload.** A pytest test (for example `tests/test_cli_contract.py`) extracts every `shatter <subcommand> [--flag ...]` invocation from the built payload files, `plugins/claude/**` and `plugins/codex/**` (SKILL.md, `references/`, `scripts/`), including fenced code blocks and inline code spans. Unshipped catalog material (design proposals, withdrawn skills) is out of scope by construction.
- [ ] **Explicit negative-example exemption.** A line or fenced block may be exempted only with an inline marker, `<!-- cli-contract: ignore -- <reason> -->` on the preceding line; the test lists every exemption it honoured in its output, and a unit test shows an unmarked bogus flag fails while a marked one passes.
- [ ] The test validates each subcommand and long flag against `shatter <sub> --help` from the pinned build and fails on an unknown subcommand or flag. Placeholders (`<...>`) and positional arguments are ignored.
- [ ] The pinned shatter commit SHA (later, a `continuous-*` BUILD tag once releases publish) is named in one file in the repo. CI obtains that binary by one of: building shatter at the pinned SHA in a job that installs Z3, Go and Node (cached), or downloading a binary artifact from a shatter CI run at that SHA. The job name and mechanism are documented in AGENTS.md.
- [ ] Locally the test skips with a visible reason when no binary is found. In CI it is required: the CI job sets `SHATTER_CONTRACT_REQUIRED=1`, and under that variable a missing binary is a failure, not a skip (a unit test covers both branches).
- [ ] Proof at close: (1) a CI run URL where the contract test executed (not skipped) and passed; (2) a local transcript of the test failing when run against `119b807` (which still ships `shatter diff --staged`), and passing on the branch.

## Suggested approach

Parse each subcommand's `--help` for the `Commands:` and `Options:` sections and cache the inventory per run. Keep the extractor conservative: only lines or spans that begin with `shatter `. Once shatter retires the snapshot `shatter diff` (<retire-snapshot-diff id>), bumping the pin makes any stray `shatter diff` mention fail.

## Out of scope

- Behavioural verification of skills (see Problem).
- Skill `status` / `requires_shatter` metadata honoured by build-plugins. That is skill-status-metadata.
- The shatter-side drift-patrol check against this catalog (str-wurp).
- Fixing individual skills: withdraw-shatter-diff-skill, recipes-marked-design-only, sa-oio.

## Dependencies

- Blocked by withdraw-shatter-diff-skill and recipes-marked-design-only: the required CI test fails while `shatter diff --staged` still ships.
- Cross-repo context (not bd dependencies): shatter <release-publish-and-install-smoke id> (downloadable pinned builds), <retire-snapshot-diff id>, str-wurp.

## Priority / Type / Labels

P2 (the verifier corrected the finding's P1 to P2: this is a missing gate, and the P1 defects it let through are filed separately) · task · testing, cli-contract, ci, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-03. Revised after the Codex cross-check (findings 3, 4, 10); the metadata half was split out to skill-status-metadata.

---

<!-- file: 06-delegate-discovery-to-engine.md -->

---
slug: delegate-discovery-to-engine
kind: new
title: "shatter-doctor should report the engine's own `shatter doctor` and move its PyYAML-dependent config check into a tested helper"
priority: P2
type: task
labels: [skills, shatter-doctor, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# shatter-doctor should report the engine's own `shatter doctor` and move its PyYAML-dependent config check into a tested helper

## Problem

The `shatter-doctor` skill diagnoses a project's Shatter setup without ever calling the engine's own diagnostic, `shatter doctor`. That command already reports the install (version, embedded frontend hashes, stale Go embed), which project config files are present, and whether generated output paths are git-ignored. The skill re-derives part of this and misses the rest.

Its one engine-independent check, parsing `.shatter/config.yaml`, is an inline `python3 -c "import sys, yaml; ..."` in SKILL.md prose. It needs PyYAML (not in the standard library), silently degrades to a byte count when PyYAML is missing, and cannot be tested because it is not code in the repo.

(The draft originally also proposed replacing `run_targets.py`'s tree walk with `shatter list-targets`. The cross-check showed that is not viable: `list-targets` returns source files, not package roots, and itself selects generated harness sources. That part became the note on sa-d8j, sa-d8j-engine-discovery-note.)

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807` and a shatter CLI built from shatter `70465921`.

- `catalog/skills/shatter-doctor/SKILL.md:71` has `python3 -c "import sys, yaml; yaml.safe_load(open(sys.argv[1]))" .shatter/config.yaml`. Lines 78-80 are the fallback "parse skipped: pyyaml not installed, file is N bytes". `catalog/skills/shatter-doctor/` has no `scripts/` directory.
- `grep -n "shatter doctor"` over `catalog/skills/shatter-doctor/SKILL.md` returns 0 matches.
- `shatter doctor --help` (args.rs:1777): `-d, --directory <DIRECTORY>`; it prints version and frontend hashes, a "Project configuration" block (presence of `shatter.config.json` and `.shatter/config.yaml`, precedence), embed staleness, and un-ignored generated paths, and exits 1 when an embed is stale or a generated path is not ignored. It does **not** parse or validate `.shatter/config.yaml`.
- The engine does not reject a malformed config either: with `.shatter/config.yaml` containing `foo: [unclosed`, `shatter list-targets --format json .` exits 0. So the skill's parse check is currently the only config validation a user gets.

## Acceptance criteria

- [ ] shatter-doctor runs `shatter doctor -d <root>` (flags as shown by the current build's `--help`) and includes its output, and its exit status, in the report. A nonzero `shatter doctor` exit is surfaced as a finding, not treated as a skill failure. The report gains the engine's embed-staleness and un-ignored-generated-path findings, which the skill does not check today; the skill's own version and config-presence steps (SKILL.md sections 1-2) may be simplified to reuse `shatter doctor` output. If the binary is missing, the existing "run install-shatter" path is unchanged.
- [ ] The config parse check moves into `catalog/skills/shatter-doctor/scripts/check_config.py` and SKILL.md calls it. It prints exactly one of: `config: missing`, `config: parsed OK`, `config: parse error at line N: <message>`, or `config: unverified (PyYAML unavailable)`. The last case is reported as unverified in the skill's summary, never as OK.
- [ ] `tests/test_shatter_doctor_config.py` runs the helper as a subprocess under `python3 -I -S` (so PyYAML is not importable) on a fixture and asserts the `unverified` line and exit 0; and, skipped with a reason when PyYAML is absent from the test environment, asserts `parsed OK` for a valid fixture and `parse error at line 1` for `foo: [unclosed`. The test fails before the change (the file does not exist).
- [ ] The helper is copied into both payloads (`find plugins -path '*shatter-doctor/scripts/check_config.py'` prints two paths).
- [ ] `python -m pytest tests/` and `scripts/check-plugins-clean` pass (output pasted in the close comment), plus a transcript of the skill's report on one real project showing the `shatter doctor` section.

## Suggested approach

Keep the helper stdlib-only apart from the optional `import yaml` inside a try block. The stale shatter-diff hook check from withdraw-shatter-diff-skill can live alongside it in the same `scripts/` directory.

## Out of scope

- Target discovery in `run_targets.py` (see sa-d8j and the sa-d8j-engine-discovery-note).
- Config validation in the engine (`shatter doctor` parsing `.shatter/config.yaml`, or commands rejecting a malformed config). That is a shatter-repo request; see the bundle's cross-repo notes.

## Dependencies

- None within this tracker. Related: withdraw-shatter-diff-skill (adds a hook check to the same skill; either can land first), cli-contract-test.

## Priority / Type / Labels

P2 · task · skills, shatter-doctor, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-10. Revised after the Codex cross-check (findings 1, 2): the run_targets.py delegation was dropped as unworkable and moved to a note on sa-d8j. Slug kept for stability although the title narrowed.

---

<!-- file: 07-sa-oio-wrapper-convention.md -->

---
slug: sa-oio-wrapper-convention
kind: note-to-existing
title: "Note on sa-oio: reconcile the wrapper convention with the default-deny execution policy; validate the subcommand in run-shatter"
priority: P2
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: []
existing_id: sa-oio
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Note on sa-oio

Target: **sa-oio** ("Generated wrappers omit CLI subcommand"), OPEN, P1. Post the comment below. It adds evidence and acceptance criteria; it does not change the issue's priority.

## Comment text

Audit 2026-09-22 (finding docs-09) adds evidence and scope to this issue. Re-verified on 2026-09-23 against shatter-agents `119b807` and a shatter CLI built from shatter `70465921`.

- `catalog/skills/add-shatter-target/SKILL.md:80` says "The wrapper command itself should be `shatter` (no extra flags)", and the package.json example at line 92 is `"shatter": "shatter"`. Bare `shatter` prints usage and exits 2 (checked against the current shatter build).
- `catalog/skills/run-shatter/scripts/run_targets.py`: `package_manager_command` (lines 114-133) returns `pnpm/yarn/bun/npm run shatter`, and `detect_integration` (around line 163) builds `task shatter`, all with no arguments; `execute_targets` runs them as-is. Nothing checks that a wrapper includes a subcommand.
- `grep -rn "allow-host-writes\|SHATTER_ALLOW_HOST_WRITES" catalog plugins` returns nothing. Shatter has enforced a default-deny host-write policy since str-gg9v (2026-07-09): executing a target is refused without an opt-in (`--allow-host-writes`, `SHATTER_ALLOW_HOST_WRITES`, or a sandbox backend). No skill tells users about that prerequisite.
- A decision is needed. This issue says "Do not inject --allow-host-writes". The audit suggested a canonical wrapper body such as `shatter scan . --allow-host-writes -o shatter-review/report.json`. Pick one, and have add-shatter-target document the execution-safety prerequisite explicitly: either the wrapper carries the opt-in, or the skill tells the user to run under a sandbox or set the variable themselves.
- Proposed additional acceptance:
  - (a) run-shatter fails with a clear message when a wrapper body has no subcommand;
  - (b) a plugin test generates a wrapper for a fixture project and runs it against a real or pinned shatter binary, asserting exit 0 and a report file;
  - (c) every subcommand and flag in the generated wrapper passes cli-contract-test.
- The shatter README's Makefile example (bare `$(SHATTER_BIN)`) is tracked separately in the shatter repo (str-dakf3).

---

<!-- file: 08-wire-shatter-ci-standalone.md -->

---
slug: wire-shatter-ci-standalone
kind: new
title: "wire-shatter-ci: template hard-codes plugin-internal run_targets.py against its own fallback guidance, Verify cannot catch it, and install.sh is fetched from main"
priority: P2
type: bug
labels: [skills, ci, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# wire-shatter-ci: template hard-codes plugin-internal run_targets.py against its own fallback guidance, Verify cannot catch it, and install.sh is fetched from main

## Problem

`catalog/skills/wire-shatter-ci/SKILL.md` generates `.github/workflows/shatter.yml` for a downstream repo. Its guidance conflicts with itself, and its Verify step cannot catch the result:

1. **The template and the companion note assume a script the downstream repo usually lacks.** Section 3 ("Determine the run command") correctly says to call `run_targets.py` only if it is vendored in the repo, and otherwise to call each target's native wrapper. But the workflow template in section 4 hard-codes `python3 scripts/run_targets.py --root . --json`, section 3 opens by recommending that same helper, and "Required companion" says the repo may reach it "through the installed Shatter plugin cache", which a GitHub runner does not have. No step vendors the file. An agent that follows the template (the most concrete instruction) produces a workflow that fails on its first run.
2. **Verify does not check that the run command can run.** Section 7 checks the `BUILD=` pin, the upload-artifact step and that "the integrated targets' run command is present", but not that any script path it references exists in the repo.
3. **The installer is not pinned.** The install step pins the binary with `BUILD: continuous-...`, then runs `curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | bash`. Fetching the installer from `main` means a future install.sh change can alter or break a supposedly pinned workflow.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807`.

- `catalog/skills/wire-shatter-ci/SKILL.md:78-91` is "3. Determine the run command": line 83 recommends `python3 scripts/run_targets.py --root . --json`; lines 86-91 say to call it only if vendored, "Otherwise call each target's native wrapper explicitly".
- `SKILL.md:131-137` is the template's install step; line 136 runs `curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | bash`.
- `SKILL.md:139-140` is the template's run step, `python3 scripts/run_targets.py --root . --json`, with no conditional.
- `SKILL.md:175-183` is "7. Verify" (BUILD pin, upload-artifact, run command present).
- `SKILL.md:195-200` is "Required companion" ("vendored in `scripts/` or invoked through the installed Shatter plugin cache").
- Context: shatter currently has no published GitHub release (`gh release list -R shatterproof-ai/shatter` is empty; tracked in the shatter repo as <release-publish-and-install-smoke id>, audit slug `release-publish-and-install-smoke`), so no `BUILD=` value resolves today. That blocks an end-to-end run, not this fix.

## Acceptance criteria

- [ ] The template's run step no longer hard-codes `scripts/run_targets.py`. It is a placeholder filled from section 3's decision: the project's native wrappers (`npm run shatter`, `task shatter`, `make shatter`) or `shatter scan`, or `run_targets.py` only when the skill has confirmed the file exists in the repo. "Required companion" no longer mentions the plugin cache as a CI option.
- [ ] Verify (section 7) additionally checks that every repo-relative script path in a `run:` step exists in the target repo, and that the install URL uses the pinned ref.
- [ ] `install.sh` is fetched from the pinned tag or commit (for example `https://raw.githubusercontent.com/shatterproof-ai/shatter/<pinned-ref>/install.sh`), derived from the same value as `BUILD`, not from `main`.
- [ ] A test in `tests/` renders the workflow as the skill instructs for two fixture repos (one with wrappers and no vendored helper, one with a vendored `scripts/run_targets.py`), parses the YAML, and asserts that every script path in a `run:` step exists in the fixture and that the install URL contains the pinned ref. The rendering logic lives in a helper under `catalog/skills/wire-shatter-ci/scripts/` so the test exercises what the skill uses. The test fails against the current template (the no-helper fixture gets `scripts/run_targets.py`).
- [ ] Optional, once shatter publishes releases: a transcript of the generated workflow running green under `act` or in a scratch GitHub repo, linked in the close comment.

## Suggested approach

Remove the dependency on `run_targets.py` from the CI path. The workflow already knows the project's wrappers, or can call `shatter scan . -o shatter-review/report.json` with the execution opt-in chosen in sa-oio. Derive the install URL from the same pinned value as `BUILD`.

## Out of scope

- Publishing shatter releases. That is shatter <release-publish-and-install-smoke id>.
- The execution-policy decision for wrappers. That is sa-oio.

## Dependencies

- None within this tracker.
- Related: sa-oio (wrapper body and execution opt-in), cli-contract-test (flags in the template).
- Cross-repo: shatter <release-publish-and-install-smoke id>, only for the optional live-run proof.

## Priority / Type / Labels

P2 · bug · skills, ci, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-11. Revised after the Codex cross-check (findings 8, 10).

---

<!-- file: 09-advise-taxonomy-payload.md -->

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

---

<!-- file: 10-claude-md-imports-agents-md.md -->

---
slug: claude-md-imports-agents-md
kind: new
title: "shatter-agents CLAUDE.md points at AGENTS.md without importing it"
priority: P2
type: bug
labels: [agent-guidance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# shatter-agents CLAUDE.md points at AGENTS.md without importing it

## Problem

`CLAUDE.md` at the root of shatter-agents tells the reader that AGENTS.md is the canonical guide, but it does not import it with `@AGENTS.md`. Claude Code loads only CLAUDE.md automatically, so Claude sessions in this repo never see AGENTS.md rules such as "never edit `plugins/` by hand" and "run `scripts/build-plugins`". Unless a session happens to open AGENTS.md itself, it can hand-edit generated payloads, and `check-plugins-clean` then fails in CI.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807`.

- `/home/ketan/project/shatter-agents/CLAUDE.md` in full:
  ```
  # Claude-specific instructions

  See [`AGENTS.md`](AGENTS.md) for the canonical agent guide. This file
  exists so Claude Code's automatic agent-doc discovery finds something
  here; the content of interest is in AGENTS.md.
  ```
  No line is `@AGENTS.md`.
- For comparison, `/home/ketan/project/bugshot/CLAUDE.md` is exactly `@AGENTS.md`.

## Acceptance criteria

- [ ] `CLAUDE.md` imports AGENTS.md: its body is `@AGENTS.md`, optionally followed by Claude-only notes.
- [ ] Proof at close: in a fresh Claude Code session started in the repo, the session can quote the build-plugins / never-edit-`plugins/` rule without reading any file. Record the question and answer in the close comment.

## Suggested approach

Replace the file body with `@AGENTS.md`, as bugshot does.

## Out of scope

A generic check for "CLAUDE.md mentions AGENTS.md without importing it" across repos. That belongs in bento's agent-env doctor (related: bento-m4y5).

## Dependencies

None.

## Priority / Type / Labels

P2 · bug · agent-guidance, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-17.

---

<!-- file: 11-close-agents-mirror-issues.md -->

---
slug: close-agents-mirror-issues
kind: new
title: "Close agents-* mirror duplicates of sa-* issues and stale-open shipped work (sa-d1b)"
priority: P2
type: chore
labels: [tracker-hygiene, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Close agents-* mirror duplicates of sa-* issues and stale-open shipped work (sa-d1b)

## Problem

The shatter-agents tracker holds 67 issues: 54 `sa-*` and 13 `agents-*`. Every `agents-*` issue has an `sa-*` twin with an identical title. This looks like a prefix rename or import that duplicated the database. Most pairs are closed on both sides, but two pairs are still live, so work appears twice in `bd ready`, `bd list` and in claims:

- `sa-xj6` / `agents-xj6` ("Add curl pipe install instruction for Codex"): both are IN_PROGRESS.
- `sa-ya6` / `agents-ya6` ("refute-from-plan skill"): both are DEFERRED.

Separately, `sa-d1b` ("Add shatter-advise and shatter-gaps skills") is still OPEN, although both skills ship in `catalog/skills/` and in the published payloads.

## Evidence

Re-verified on 2026-09-23 with `bd list --all --json` in `/home/ketan/project/shatter-agents`: 67 issues in total (54 `sa-`, 13 `agents-`). There are 13 title-identical cross-prefix pairs: 3lu, 8d6, 1fc, 08h, xj6, fqc, b33, smh, 16f, 4z3, 4tm, gre and ya6. Of these, xj6 is in_progress on both sides and ya6 is deferred on both sides; the rest are closed on both sides.

- `bd show sa-d1b` shows OPEN, P2. `catalog/skills/shatter-advise/` and `catalog/skills/shatter-gaps/` exist and are listed in `catalog/plugins.json`.

## Acceptance criteria

- [ ] Each non-closed `agents-*` issue that has a title-identical `sa-*` twin (currently agents-xj6 and agents-ya6) is closed as a duplicate, with a reason naming the twin. The closed/closed pairs may be left alone or annotated. The one-liner used for the check, run after the cleanup, prints no pair in which both sides are non-closed. Paste its output in the close comment.
- [ ] `sa-d1b` is closed with evidence (the skill paths and the landing commit), or narrowed to whatever part is still unshipped.
- [ ] `sa-xj6`'s claim state is reconciled: either it is in_progress with a live owner and a recent update, or it is released back to open.
- [ ] The tracker changes are persisted using the repo's current beads sync convention. Do not hand-edit `.beads/issues.jsonl`. Bento's beads-issue-flow is getting Dolt-remote sync guidance from this audit (D4), so follow whatever it says at the time of closing.

## Suggested approach

Run `bd list --all --json`, group by title, and close the `agents-*` side with `bd close <id> --reason "duplicate of sa-<x>"` (check `bd close --help` for the duplicate flag). Look at `sa-xj6` against README history to decide whether it has shipped.

## Out of scope

- A generic cross-prefix duplicate detector. That belongs in bento beads-issue-flow.
- The bugshot half of the same finding (bugshot-* and bgs-* mirrors), which is filed in the bugshot tracker.

## Dependencies

None.

## Priority / Type / Labels

P2 · chore · tracker-hygiene, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-16 (the shatter-agents half).

---

<!-- file: 12-skill-status-metadata.md -->

---
slug: skill-status-metadata
kind: new
title: "Skill metadata.json status/requires_shatter fields that build-plugins honours, so unreleased skills can live in the catalog without shipping"
priority: P3
type: feature
labels: [packaging, build-plugins, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Skill metadata.json status/requires_shatter fields that build-plugins honours, so unreleased skills can live in the catalog without shipping

## Problem

There is no way to keep a skill in `catalog/plugins.json` while marking it as documenting a future shatter command. `catalog/skills/*/metadata.json` carries only `recommended_model` and `audience`, and `scripts/build-plugins` has no status or requires handling, so every listed skill ships. The two P1 withdrawals from this audit (withdraw-shatter-diff-skill, recipes-marked-design-only) work around this by deleting the skill or dropping it from `plugins.json`. That is enough for now; this mechanism is useful the next time a skill is written ahead of engine support (for example a diff-scoped exploration skill once str-81xiw is under way). It is independent of cli-contract-test, which only reads the built payload and therefore works either way.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807`.

- `grep -n "status\|requires" scripts/build-plugins` returns 0 matches.
- `scripts/build-plugins` `_is_ignored` (around line 83) skips only `__pycache__` and `.pyc`; `load_catalog` (around line 128) loads every skill named in `plugins.json`.
- Example metadata: `catalog/skills/shatter-advise/metadata.json` has only `recommended_model` and `audience`.

## Acceptance criteria

- [ ] `metadata.json` accepts optional `status` (`released` | `experimental` | `unreleased`, default `released`) and `requires_shatter` (a shatter commit/BUILD tag or an upstream issue id). An unknown `status` value is a build error.
- [ ] `scripts/build-plugins` excludes `unreleased` skills from both the Claude and Codex payloads; `experimental` ships with a banner line prepended to the composed SKILL.md naming `requires_shatter`.
- [ ] `tests/test_build_plugins.py` builds a fixture catalog with one skill of each status and asserts: the unreleased skill is absent from both payloads, the experimental one carries the banner, the released one is unchanged. The test fails before the change.
- [ ] AGENTS.md documents the fields in one short paragraph.
- [ ] `scripts/check-plugins-clean` and `python -m pytest tests/` pass (output pasted in the close comment).

## Out of scope

- Re-shipping shatter-diff or compose-shatter-recipe under the new status. Those are new issues once the engine features exist.

## Dependencies

- None. Related: cli-contract-test, withdraw-shatter-diff-skill, recipes-marked-design-only.

## Priority / Type / Labels

P3 · feature · packaging, build-plugins, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-03 (metadata half). Split from cli-contract-test after the Codex cross-check (finding 4).

---

<!-- file: 13-sa-d8j-engine-discovery-note.md -->

---
slug: sa-d8j-engine-discovery-note
kind: note-to-existing
title: "Note on sa-d8j: `shatter list-targets` is not a drop-in replacement for run_targets.py discovery; fix the prune locally"
priority: P2
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: []
existing_id: sa-d8j
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Note on sa-d8j

Target: **sa-d8j** ("Generated harnesses inflate target discovery"), OPEN, P2. Post the comment below. It adds evidence against one tempting fix; it does not change the issue's scope or priority.

## Comment text

Audit 2026-09-22 (finding plugins-10) proposed fixing this by having `run_targets.py` take its roots from `shatter list-targets --format json` instead of walking the tree. The cross-check showed that does not work today, so keep the fix local (prune `.shatter/` at every depth in `should_exclude_dir`, as this issue already requests). Evidence, re-verified on 2026-09-23 against shatter-agents `119b807` and a shatter CLI built from shatter `70465921`:

- `shatter list-targets` "lists source files that would be selected for a scan". Its JSON (`kind: target_manifest`) has one `project_root` and `selected[]` / `excluded[]` / `unsupported[]` / `candidate_outside_policy[]` arrays of individual **source files** (`path`, `language`, `frontend`). It has no notion of package roots (Cargo.toml / go.mod / package.json directories), which run-shatter needs to find wrappers.
- Against the existing fixture `tests/fixtures/run-targets/mixed-repo` (manifests only, no sources), `list-targets` returns `selected: []`, while `tests/test_run_targets.py:39` expects three roots (`go-service`, `rust-lib`, `ts-app`).
- `list-targets` does not exclude generated harnesses either. In a temp tree with `src/lib.rs` and `.shatter/cache/harness/src/lib.rs`, `selected[].path` is `['.shatter/cache/harness/src/lib.rs', 'src/lib.rs']`. (The engine's native glob walker excludes `.shatter` — `shatter-cli/src/args.rs` `GLOB_WALK_EXCLUDE_DIRS` — but list-targets' discovery does not.) That engine-side gap belongs in the shatter tracker, not here.

Close proof for this issue stays as its own description says: the minimal-fixture test (root `Cargo.toml` plus `.shatter/cache/harness/rust/bin-only/<id>/Cargo.toml`, and the nested `api/` variant) fails on current code and passes after the prune. Delegating discovery to the engine can be reconsidered only if shatter adds a package-root listing that excludes managed state; that would be a new issue.

sa-c2q (Make wrappers invisible) is unaffected: wrapper detection stays in `run_targets.py` either way.
