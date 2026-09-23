# Bundle: shatter-agents-plugin

- **Bucket:** shatter-agents-plugin. Plugin skills that document CLI behaviour the engine does not have, plus contract tests, delegation, CI wiring and payload hygiene.
- **Repo / tracker:** shatter-agents. bd in /home/ketan/project/shatter-agents, prefix `sa` (the config has no issue-prefix; the prefix is taken from existing ids such as sa-tyb).
- **Parent epic:** "Epic: Audit 2026-09-22 findings (shatter-agents plugin)".
- **Contents:** 11 entries. 8 are new issues (2 at P1, 6 at P2), 2 are reopen-notes on closed issues (sa-tyb, sa-yyt) and 1 is a note on an existing issue (sa-oio). Nothing has been filed.
- **Verification basis:** shatter-agents `119b807` and shatter audit worktree HEAD (`target/debug/shatter`), re-checked on 2026-09-23.

## Maintainer decisions (2026-09-23)

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix. Fix them (the Z3 header or static link on Windows; openssl-sys under cross for aarch64) rather than dropping them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire the snapshot-diff command (`shatter diff`) and the unused Snapshot writer path. spec-diff is THE regression tool, and SPEC, README and QUICKSTART are updated to match. Whether diff-scoped exploration (str-81xiw) later takes the `diff` name is left to str-81xiw. The plugin's `shatter diff --staged` docs must be corrected to what exists today. *(Applied in 01 and 02.)*
- **D3 Concolic positioning:** measure first. A P1 default-vs-concolic benchmark, a P1 fix for concolic early termination, and then a follow-up decision issue. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import in shatter and move tracker sync to a Dolt remote. AGENTS.md drops `bd sync`, str-qwua7.28 is superseded, and bento beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT fix and no hook-bypass guidance. *(Touched only by 11, which tells implementers to follow the current sync convention and not to hand-edit the JSONL.)*
- **D5 Git identity:** the leaked `[user]` section was already removed. Add a .mailmap, a git-state drift check and a fixture `.git/config` snapshot test. *(Not applicable to this bucket.)*
- **D6 Filing:** after reconciliation and the Codex cross-check, the maintainer runs one filer script. Nothing is filed by agents.

## Cross-repo references (not bd dependencies)

- The shatter-repo slug `retire-snapshot-diff` (bucket shatter-artifacts-correctness) and shatter epic str-81xiw provide context for 01, 02 and 05.
- The shatter-repo slug `release-publish-and-install-smoke` (bucket shatter-ci-workflows) provides context for 05 and 08: shatter has no published release today, so pinned `BUILD=` downloads do not resolve.
- In reopen-note and note text, `<slug id>` placeholders are replaced by the filer with the new issue ids.

## Index

| # | Slug | Kind | P | Title |
|---|---|---|---|---|
| 01 | withdraw-shatter-diff-skill | new | P1 | Withdraw the shatter-diff skill: it documents a nonexistent `shatter diff <base-ref> --staged` command, and its pre-commit hook recipe blocks every commit |
| 02 | sa-tyb-reopen-note | reopen-note (sa-tyb) | P1 | Comment on closed sa-tyb: only the skill text landed; the documented command does not exist |
| 03 | recipes-marked-design-only | new | P1 | compose-shatter-recipe and run-shatter document recipes and a stubs: registry that neither the engine nor run_targets.py implements |
| 04 | sa-yyt-reopen-note | reopen-note (sa-yyt) | P1 | Comment on closed sa-yyt: recipe discovery and the stubs registry are documented but not implemented |
| 05 | cli-contract-test | new | P2 | Add a contract test between catalog skills and a pinned shatter CLI, plus status/requires metadata that build-plugins honours |
| 06 | delegate-discovery-to-engine | new | P2 | run-shatter and shatter-doctor should call `shatter list-targets` / `shatter doctor` instead of reimplementing discovery |
| 07 | sa-oio-wrapper-convention | note-to-existing (sa-oio) | P2 | Note on sa-oio: reconcile the wrapper convention with the default-deny execution policy; validate the subcommand in run-shatter |
| 08 | wire-shatter-ci-standalone | new | P2 | wire-shatter-ci generates a workflow that needs plugin-internal run_targets.py and fetches install.sh from main |
| 09 | advise-taxonomy-payload | new | P2 | shatter-advise/shatter-gaps cite a taxonomy spec that is not shipped; stale agents-arz id; test file shipped in the payload |
| 10 | claude-md-imports-agents-md | new | P2 | shatter-agents CLAUDE.md points at AGENTS.md without importing it |
| 11 | close-agents-mirror-issues | new | P2 | Close agents-* mirror duplicates of sa-* issues and stale-open shipped work (sa-d1b) |

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

Audit 2026-09-22 (finding plugins-01): this issue was closed when the `shatter-diff` skill text landed, but the command the skill documents does not exist. `shatter diff --staged` fails with `error: unexpected argument '--staged' found`, and `Usage: shatter diff [OPTIONS] <SNAPSHOT> <CURRENT>` exits 2. `shatter diff-explore` is an unrecognized subcommand. As a result, the skill's pre-commit hook recipe (`catalog/skills/shatter-diff/SKILL.md:122-136`) aborts every commit.

Current state:
- The engine work for diff-scoped exploration is shatter epic **str-81xiw** (open). It has not shipped.
- On 2026-09-23 the shatter maintainer decided to retire the snapshot-comparison `shatter diff`. `shatter spec-diff` is the supported regression tool. Once the retirement lands, the `diff` name is free, and str-81xiw decides what the diff-scoped command is called.

Follow-up: **<withdraw-shatter-diff-skill id>** withdraws the skill and its hook recipe from the published payload and points readers to `shatter spec-diff`. A new skill for diff-scoped exploration should be filed once str-81xiw ships a command, and should be closed only after the skill's commands have been run against a real build.


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

Follow-up: **<recipes-marked-design-only id>** marks the recipe material as a design proposal (or withdraws it), removes the per-recipe-run claim from run-shatter, and links a shatter-repo issue for the engine side. sa-mty (the design) stays closed.


---

<!-- file: 05-cli-contract-test.md -->

---
slug: cli-contract-test
kind: new
title: "Add a contract test between catalog skills and a pinned shatter CLI, plus status/requires metadata that build-plugins honours"
priority: P2
type: task
labels: [testing, cli-contract, ci, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Add a contract test between catalog skills and a pinned shatter CLI, plus status/requires metadata that build-plugins honours

## Problem

Nothing checks that the `shatter` subcommands and flags cited in shatter-agents skills exist in the shatter CLI. shatter-agents CI runs only `scripts/check-plugins-clean` and `python -m pytest tests/`, and no test invokes a shatter binary. The shatter repo does not reference shatter-agents at all, and its own CLI-surface drift check (str-wurp, still open) covers only shatter's SPEC.md and gauntlet. As a result, skills that cite nonexistent commands shipped undetected: `shatter diff --staged` (withdraw-shatter-diff-skill) and per-recipe runs (recipes-marked-design-only).

There is also no way to mark a skill as documenting a future command. `catalog/skills/*/metadata.json` carries only `recommended_model` and `audience`, and `scripts/build-plugins` has no status or requires handling. So every catalog skill listed in `catalog/plugins.json` ships.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807`.

- `.github/workflows/ci.yml` has two jobs: `build-clean` (`scripts/check-plugins-clean`) and `tests` (`python -m pytest tests/ -v`).
- `tests/` has 13 test files (for example `test_skills_load.py` and `test_run_targets.py`). None runs `shatter` or parses `--help`.
- `grep -n "status\|requires" scripts/build-plugins` returns 0 matches. `_is_ignored` (around line 83) skips only `__pycache__` and `.pyc`.
- In the shatter repo, `grep -r "shatter-agents\|shatterproof-ai/agents"` (excluding `target/`, `audits/` and `.beads/`) finds nothing. The `cli-surface-drift` check in `docs/DRIFT-PATROL.md` is still pending on str-wurp.
- The shatter project has **no published GitHub release**: `gh release list -R shatterproof-ai/shatter` is empty. It is tracked in the shatter repo as `release-publish-and-install-smoke`. For now, a pinned binary therefore has to be built from a pinned shatter commit, not downloaded.

## Acceptance criteria

- [ ] A pytest test (for example `tests/test_cli_contract.py`) extracts every `shatter <subcommand> [--flag ...]` invocation from `catalog/**/*.md` and `catalog/**/scripts/*`, including fenced code blocks and inline code spans. It then validates each subcommand and long flag against `shatter <sub> --help` from a pinned shatter build, and fails on an unknown subcommand or flag. Placeholders (`<...>`) and positional arguments are ignored.
- [ ] The pinned build is named in one place in the repo (a shatter commit SHA, or a `continuous-*` BUILD tag once releases publish). CI obtains that binary, either by building shatter at the pinned SHA with a cache or by downloading the pinned release. The test is skipped with a visible reason when no binary is available locally, but it is **required** in CI.
- [ ] Skill `metadata.json` accepts `status` (`released` | `experimental` | `unreleased`) and `requires_shatter` (a minimum build or an upstream issue id). `scripts/build-plugins` excludes non-released skills from both the Claude and Codex payloads. A test in `tests/test_build_plugins.py` proves the exclusion.
- [ ] Proof at close: link a CI run in which the contract test executed (not skipped) and passed. Also include a local transcript showing the test failing against a checkout that still ships the `shatter diff --staged` text (before withdraw-shatter-diff-skill), or failing on a deliberately injected bogus flag.

## Suggested approach

Parse each subcommand's `--help` output for the `Commands:` and `Options:` sections, and cache the resulting inventory per run. Keep the extractor conservative: only lines or spans that begin with `shatter `. Start the CI job by building `shatter-cli` at the pinned SHA with `cargo build -p shatter-cli --release` and a cargo cache, and switch to `install.sh` with `BUILD=` once shatter publishes releases.

Also, once shatter retires the snapshot `shatter diff` (shatter `retire-snapshot-diff`), bumping the pin should make any stray `shatter diff` mention fail this test.

## Out of scope

- The shatter-side drift-patrol check against this catalog. That is an optional advisory in the shatter repo (str-wurp).
- Fixing the individual skills. Those are withdraw-shatter-diff-skill, recipes-marked-design-only and sa-oio.

## Dependencies

- None within this tracker. The status/requires mechanism can land first, and withdraw-shatter-diff-skill may reuse it.
- Cross-repo context: shatter `release-publish-and-install-smoke` (downloadable pinned builds) and str-wurp.

## Priority / Type / Labels

P2 (the verifier corrected the finding's P1 to P2: this is a missing gate, and the P1 defects it let through are filed separately) · task · testing, cli-contract, ci, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-03.


---

<!-- file: 06-delegate-discovery-to-engine.md -->

---
slug: delegate-discovery-to-engine
kind: new
title: "run-shatter and shatter-doctor should call `shatter list-targets` / `shatter doctor` instead of reimplementing discovery"
priority: P2
type: task
labels: [skills, run-shatter, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# run-shatter and shatter-doctor should call `shatter list-targets` / `shatter doctor` instead of reimplementing discovery

## Problem

`catalog/skills/run-shatter/scripts/run_targets.py` walks the tree for `Cargo.toml`, `go.mod` and `package.json` itself, instead of asking the engine which targets it sees. Two open bugs come straight from this parallel discovery:

- sa-d8j: the generated `.shatter/cache/harness/Cargo.toml` is counted as a target.
- sa-c2q: Make wrappers are invisible to run-shatter.

`catalog/skills/shatter-doctor/SKILL.md` validates `.shatter/config.yaml` with `python3 -c "import sys, yaml; ..."`. That needs PyYAML, which is not in the Python standard library, so the skill falls back to a byte count when PyYAML is missing. The skill never calls the engine's own `shatter doctor` or `shatter list-targets`.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807` and shatter audit HEAD.

- `catalog/skills/shatter-doctor/SKILL.md:71` has `python3 -c "import sys, yaml; yaml.safe_load(open(sys.argv[1]))" .shatter/config.yaml`. Lines 78-80 are the fallback "parse skipped: pyyaml not installed".
- `grep -n "shatter doctor\|list-targets"` over `catalog/skills/shatter-doctor/SKILL.md`, `catalog/skills/run-shatter/SKILL.md` and `run_targets.py` returns 0 matches.
- shatter `shatter-cli/src/args.rs:1767` defines `ListTargets(ListTargetsArgs)`, with `--format` (`ListTargetsFormat`, including json) at around line 1810. Line 1777 defines `Doctor`, which checks embed staleness and gitignore coverage. It does **not** validate config.yaml, so it complements the skill's config check rather than replacing it.
- sa-d8j (P2) is OPEN, and sa-c2q (P1) and sa-oio (P1) are OPEN.

## Acceptance criteria

- [ ] shatter-doctor runs `shatter doctor -d <root>` and `shatter list-targets --format json` (flags as shown by the current build's `--help`) and reports their output. It keeps only the config checks the engine does not cover, and those checks need no third-party Python module. "Uses only the standard library" is enforced by a test that runs the check under `python3 -S -I` or with PyYAML absent.
- [ ] `run_targets.py` gets target roots from `shatter list-targets --format json`. Its own file walking is removed, and it keeps only wrapper detection (package.json, Taskfile, Makefile) as the integration layer on top of engine targets.
- [ ] A regression test in `tests/test_run_targets.py` builds a fixture that contains `.shatter/cache/harness/Cargo.toml` and asserts that the file is not reported as a target. The test fails on the current code and passes after the change, which closes sa-d8j. If it cannot, sa-d8j is updated to say why.
- [ ] sa-c2q is updated to say whether engine-based discovery resolves it.
- [ ] `python -m pytest tests/` and `scripts/check-plugins-clean` pass (output pasted in the close comment).

## Suggested approach

Add a small helper in `run_targets.py` that shells out to `shatter list-targets --format json` and maps the returned roots to wrappers. Keep a clear error when the `shatter` binary is missing, because shatter-doctor already reports that. In tests, stub the binary with a fixture script that prints canned JSON, and put one real-binary case behind the cli-contract-test pin.

## Out of scope

- The wrapper subcommand fix (sa-oio; see the sa-oio-wrapper-convention note).
- Adding config.yaml validation to `shatter doctor` in the engine. If that is wanted, it is a shatter-repo issue.

## Dependencies

- None within this tracker.
- Related: sa-d8j, sa-c2q, sa-oio, cli-contract-test.

## Priority / Type / Labels

P2 · task · skills, run-shatter, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-10.


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

Audit 2026-09-22 (finding docs-09) adds evidence and scope to this issue. Re-verified on 2026-09-23 against shatter-agents `119b807`.

- `catalog/skills/add-shatter-target/SKILL.md:80` says "The wrapper command itself should be `shatter` (no extra flags)", and the package.json example at line 92 is `"shatter": "shatter"`. Bare `shatter` prints usage and exits 2 (checked against the current shatter build).
- `catalog/skills/run-shatter/scripts/run_targets.py:119-133` invokes `pnpm/yarn/bun/npm run shatter`, and line 163 invokes `task shatter`, all with no arguments. Nothing checks that a wrapper includes a subcommand.
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
title: "wire-shatter-ci generates a workflow that needs plugin-internal run_targets.py and fetches install.sh from main"
priority: P2
type: bug
labels: [skills, ci, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# wire-shatter-ci generates a workflow that needs plugin-internal run_targets.py and fetches install.sh from main

## Problem

`catalog/skills/wire-shatter-ci/SKILL.md` generates `.github/workflows/shatter.yml` for a downstream repo. The generated workflow has two defects:

1. **It depends on a script the downstream repo does not have.** The run step is `python3 scripts/run_targets.py --root . --json`. `run_targets.py` lives inside the plugin (`catalog/skills/run-shatter/scripts/`). The skill's "Required companion" section says the repo "must have access to it (either vendored in scripts/ or invoked through the installed Shatter plugin cache)". A GitHub runner has no plugin cache, no skill step vendors the file, and the skill's Verify step checks only the `BUILD=` pin and the upload-artifact step. The generated workflow therefore fails on its first run.
2. **The installer is not pinned.** The install step sets `BUILD: continuous-...` to pin the binary, but then runs `curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | bash`. That fetches the installer script from `main`, so a future install.sh change can alter or break a supposedly pinned workflow.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807`.

- `catalog/skills/wire-shatter-ci/SKILL.md:65-72` is "Choose a pinned `BUILD=` tag" (`BUILD=continuous-YYYYMMDD-HHMM-<sha>`).
- `SKILL.md:131-137` is the install step, which runs `curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | bash`.
- `SKILL.md:139-140` is the run step, `python3 scripts/run_targets.py --root . --json`.
- `SKILL.md:175` is "7. Verify" (it checks BUILD and upload-artifact only).
- `SKILL.md:195-200` is "Required companion" ("vendored in scripts/ or invoked through the installed Shatter plugin cache").
- Context: shatter currently has no published GitHub release (`gh release list -R shatterproof-ai/shatter` is empty; tracked in the shatter repo as `release-publish-and-install-smoke`), so no `BUILD=` value resolves today. That is a shatter-side blocker for running the workflow end to end, not a defect in this skill.

## Acceptance criteria

- [ ] The generated workflow does not reference any file the skill has not written into the target repo. Preferred: it runs the project's own wrappers (`npm run shatter`, `task shatter`, `make shatter`) or native `shatter scan` directly. Alternatively, the skill vendors `run_targets.py` with a version header and a documented update path, as an explicit step that its Verify section checks.
- [ ] `install.sh` is fetched from the pinned tag or commit (for example `https://raw.githubusercontent.com/shatterproof-ai/shatter/<pinned-ref>/install.sh`), not from `main`.
- [ ] A test in `tests/` renders the workflow template for a fixture repo, parses the YAML, and asserts that every script path referenced in a `run:` step exists in the fixture after the skill's steps, and that the install URL contains the pinned ref. The test fails against the current template.
- [ ] Optional, once shatter publishes releases: a transcript of the generated workflow running green under `act` or in a scratch GitHub repo, linked in the close comment.

## Suggested approach

Remove the dependency on `run_targets.py` from the CI path. The workflow already knows the project's wrappers, or can call `shatter scan . -o shatter-review/report.json` with the execution opt-in chosen in sa-oio. Derive the install URL from the same pinned value as `BUILD`.

## Out of scope

- Publishing shatter releases. That is shatter `release-publish-and-install-smoke`.
- The execution-policy decision for wrappers. That is sa-oio.

## Dependencies

- None within this tracker.
- Related: sa-oio (wrapper body and execution opt-in), cli-contract-test (flags in the template).
- Cross-repo: shatter `release-publish-and-install-smoke`, only for the optional live-run proof.

## Priority / Type / Labels

P2 · bug · skills, ci, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-11.


---

<!-- file: 09-advise-taxonomy-payload.md -->

---
slug: advise-taxonomy-payload
kind: new
title: "shatter-advise/shatter-gaps cite a taxonomy spec that is not shipped; stale agents-arz id; test file shipped in the payload"
priority: P2
type: bug
labels: [skills, packaging, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# shatter-advise/shatter-gaps cite a taxonomy spec that is not shipped; stale agents-arz id; test file shipped in the payload

## Problem

shatter-advise and shatter-gaps produce findings that cite named patterns by `pattern_id`. The skill text says those patterns are defined in `docs/specs/2026-06-16-shatter-tractability-taxonomy.md` "(in the shatter-agents repo)". That file exists only in the source repo and is not part of either published payload. An installed agent therefore cannot read the catalog its own findings cite.

The skill also references the tracker ids `agents-arz` (the live issue is `sa-arz`, deferred) and `agents-2b3`. Tracker ids do not belong in shipped skill text in any case.

Finally, `scripts/build-plugins` copies `scripts/test_discover_hotspots.py` into the published payload, because its ignore rule covers only `__pycache__` and `.pyc`.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807`.

- `catalog/skills/shatter-advise/SKILL.md:317` reads "The taxonomy spec at `docs/specs/2026-06-16-shatter-tractability-taxonomy.md` …". `SKILL.md:319` reads "The async-shell/sync-core pattern (agents-arz) is also first-class", and `SKILL.md:330` cites `agents-2b3`. `grep -rnoE "\b(agents|sa)-[a-z0-9]{3}\b" plugins/` finds exactly these two ids, in both the Claude and Codex copies of shatter-advise.
- `docs/specs/2026-06-16-shatter-tractability-taxonomy.md` exists in the repo (21.9 KB).
- `plugins/claude/shatter/skills/shatter-advise/` contains `SKILL.md`, `metadata.json` and `scripts/` (`discover_hotspots.py`, `test_discover_hotspots.py`). `plugins/claude/shatter/skills/shatter-gaps/` contains `SKILL.md` and `metadata.json`. The Codex payload mirrors this.
- `scripts/build-plugins` around line 83 has `_is_ignored(p) = "__pycache__" in parts or suffix == ".pyc"`.

## Acceptance criteria

- [ ] The taxonomy ships as a companion reference, for example `references/taxonomy.md`, under shatter-advise, with shatter-gaps citing it by a relative path that resolves in the built payload (or both skills carry or share one copy). The source spec and the shipped copy cannot drift: either the build copies it, or a test compares them.
- [ ] No tracker ids (`agents-*` or `sa-*`) appear in shipped skill text. `grep -rnE "\b(agents|sa)-[a-z0-9]{3}\b" plugins/` returns nothing.
- [ ] `scripts/build-plugins` excludes `test_*.py` (and `*_test.py`) from payloads. `tests/test_build_plugins.py` asserts this, and `find plugins -name 'test_*.py'` is empty.
- [ ] A smoke test reads each built SKILL.md from `plugins/claude/...` and `plugins/codex/...` and asserts that every relative path it references exists inside that skill's payload directory.
- [ ] `scripts/check-plugins-clean` and `python -m pytest tests/` pass (output pasted in the close comment), and the plugin version is bumped.

## Suggested approach

Use the existing companion-files mechanism (`_collect_companion_files` handles `scripts/`, so add `references/`). Copy the spec at build time rather than hand-maintaining a second copy. Replace "(agents-arz)" with a plain description of the pattern.

## Out of scope

- Changing the taxonomy's content.
- sa-d1b bookkeeping. That is close-agents-mirror-issues.

## Dependencies

- None within this tracker. Related: sa-d1b, sa-3lu, sa-arz.

## Priority / Type / Labels

P2 · bug · skills, packaging, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-14.


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
