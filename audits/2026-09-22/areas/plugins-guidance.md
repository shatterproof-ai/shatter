# Area review: other first-party agent components (plugins-guidance)

Audit 2026-09-22, read-only. Reviewer scope: bugshot (`~/project/bugshot`), storystore
(`~/project/storystore`), the shatter plugin marketplace (`~/project/shatter-agents`),
global agent guidance (`~/.claude/CLAUDE.md`, `~/dotfiles/codex/AGENTS.md`,
`~/dotfiles/docs/agent-guidance*`, `code-writing-guidance*`,
`source-code-management-guidance.md`) and the agent-env-doctor dormancy nag.

Bento-internal items (doctor per-worktree state, preview leaks from bento's test suite,
orphan worktree directories) are covered in `areas/bento.md` F8, F9 and F10. This file
does not repeat them.

## 0. What changed since the 2026-09-04 audit

| Prior item | Status 2026-09-22 | Evidence |
|---|---|---|
| dotfiles #9-#13 (routing to dotfiles AGENTS.md, subagent/tool reconciliation, rtk narrowing, grep shadow, bd prime claim) | all closed 09-07/08 | `gh issue list` in ~/dotfiles |
| rtk prefilter | live: `~/.claude/settings.json` PreToolUse Bash -> `python3 $DOTFILES/claude/rtk_prefilter.py`; the file has 296 lines of tests (`claude/tests/test_rtk_prefilter.py`) | settings dump |
| bento doctor repo-state checks + seen/remind_after | shipped (bento-rdtn closed): `agent-env-doctor.py` has `check_bare_primary`, `check_prunable_worktrees`, `check_stale_previews`, `check_worktree_root_orphans`, `_dormant_plugin_decisions` with seen/remind_after | catalog/hooks/bento/claude/scripts/agent-env-doctor.py:482-559, 676-887 |
| primary checkout `core.bare=true` (str-qwua7.1) | **repaired**: `git rev-parse --is-inside-work-tree` -> `true`, `core.bare` -> `false` (file:.git/config). But str-qwua7.1 is still open and the memory `project_shatter_gate_cache_and_bare_primary.md` still says bare=true | see F16 |
| storystore decision (str-qwua7.52: adopt) | not started; `docs/stories/` absent; no skip key | F8 |
| bugshot decision (str-qwua7.53: set skip key now, wire after bgs-3tq) | skip key never set; bgs-3tq open at **P3** in bugshot | F8 |
| memory hygiene (prior rec 28/29) | not done: pickpackit/kapow/zolem memories (~28 KB) still in the shatter memory dir | F16 |

## 1. Shatter plugin (shatter-agents) vs the current shatter CLI

Installed: `shatter@shatterproof` 0.1.1, gitCommitSha efa59682 (2026-06-18).
Source: `plugins/claude/shatter/.claude-plugin/plugin.json` version 0.1.12, HEAD 119b807.
`git rev-list --count efa59682..HEAD` = 27. The `shatterproof` entry in `known_marketplaces.json`
has `lastUpdated 2026-06-19` and no `autoUpdate` (bento has `"autoUpdate": true`). So every shatter
session loads a three-month-old plugin. It does not have `compose-shatter-recipe` or `shatter-diff`
(check the skill list in this session). Given F1 and F2, that is partly lucky.

### F1 shatter-diff documents a command that does not exist (P1, L2)
`catalog/skills/shatter-diff/SKILL.md` documents
`shatter diff [<base-ref>] [--staged] [--include-tests] [--output-dir <path>] [--format json|text] [--jobs <N>]`
and gives a pre-commit hook built on it. Real CLI (`target/debug/shatter`, built from audit HEAD):

```
$ shatter diff --staged --format json --output-dir /tmp/x
error: unexpected argument '--staged' found
Usage: shatter diff [OPTIONS] <SNAPSHOT> <CURRENT>
exit=2
$ shatter diff main
error: the following required arguments were not provided: <CURRENT>
exit=2
$ shatter diff-explore
error: unrecognized subcommand 'diff-explore'
```

`shatter-cli/src/args.rs:1432-1444` defines `Diff { snapshot, current, json }` (snapshot comparison).
The skill's hook example has `if ! shatter diff --staged ...; then echo "commit aborted"; exit 1`.
A downstream user who installs it gets **every commit blocked**.
History: sa-tyb, "shatter diff: first-class diff-scoped exploration command", was closed with
"a850d93 landed on main". That commit landed the *skill*. The engine work moved to shatter epic
str-81xiw ("Migrated from mistakenly filed shatter-agents issue sa-tyb"). That epic renamed the command
`shatter diff-explore` because `diff` is taken, and it is still open (str-81xiw.2 CLI shell open).
The skill was never withdrawn or re-gated.

### F2 compose-shatter-recipe and run-shatter "recipes" describe engine features that do not exist (P1, L2)
- `compose-shatter-recipe/SKILL.md:145-239` covers `.shatter/recipes/<target-id>/*.json`, a `stubs:`
  section in `.shatter/config.yaml` with `implements/lang/source/factory`, and resolver errors such as
  `unsupported recipe schemaVersion <n>` (:342).
- `run-shatter/SKILL.md:~70-100` says run-shatter "discovers and runs" recipes: once per recipe,
  validated before the run.
- The code has none of this. `grep -i recipe` over `catalog/skills/run-shatter/scripts/*.py` finds
  0 matches (360 lines). In shatter-core/cli/rust src, "recipe" only appears in unrelated
  reconstruction-recipe protocol fields. No config struct has `stubs`.
- sa-yyt, "implement: recipe schema and stub-registration...", is closed with the reason
  "compose-shatter-recipe skill landed on main (3b7aec4)". It is the same pattern as F1.

### F3 No contract test between plugin skills and the shatter CLI (P1, AGENT)
`shatter-agents/.github/workflows/ci.yml` runs only `scripts/check-plugins-clean` and `pytest tests/`.
Nothing extracts `shatter <subcommand> <flags>` from `catalog/` and checks it against the CLI. On the
shatter side, `grep -r 'shatter-agents\|shatterproof-ai/agents'` finds no references outside
`audits/`, and drift-patrol `cli-surface` is still PENDING (str-wurp). The global
`wiring-and-consumption.md` rule ("A capability is complete only when production code invokes it") and
`drift-checks.md` ("Compare runtime command inventory ... against README") would have caught F1 and F2.
F12 shows neither is ever loaded.

### F4 Downstream plugin reimplements engine discovery and doctor (P2, L4)
- `run-shatter/scripts/run_targets.py` walks for Cargo.toml/go.mod/package.json on its own and treats
  "integrated" as "a wrapper named shatter exists". Open bugs sa-d8j (`.shatter/cache/harness/...`
  counted as targets) and sa-c2q (Make wrappers invisible) come from this parallel discovery.
  The engine already has `shatter list-targets [--format json]` (`args.rs:1766`) and `scan`.
- `shatter-doctor/SKILL.md:37-71` checks `shatter --version` and parses `.shatter/config.yaml` with
  `python3 -c "import yaml ..."`, which needs PyYAML and is not stdlib. It never calls `shatter doctor`,
  which exists (`args.rs:1777`) and checks embed staleness and gitignore coverage.
- sa-oio (bare `shatter` in generated wrappers exits 2) is still true: `add-shatter-target/SKILL.md:92,112,125`.

### F5 wire-shatter-ci emits a workflow that runs a file downstream repos don't have (P2, L2/L6)
The template `wire-shatter-ci/SKILL.md:140` runs `python3 scripts/run_targets.py --root . --json`.
`run_targets.py` is plugin-internal, and "Required companion" (:196-200) says "the user's repo must
have access to it (either vendored ... or ... plugin cache)". There is no plugin cache on a GitHub runner.
No workflow step vendors the file, and step 7 (Verify) does not check that it exists. Also,
"Install shatter (pinned)" pins `BUILD=` but fetches `install.sh` from `main` (:136), so the
installer itself is not pinned.

### F6 shatter-advise points at a spec the plugin does not ship; stale IDs; test file shipped (P2, L3)
- `shatter-advise/SKILL.md:317` says "The taxonomy spec at `docs/specs/2026-06-16-shatter-tractability-taxonomy.md`
  (in the shatter-agents repo)". That file is not in `plugins/claude/shatter/skills/shatter-advise/`,
  so installed agents cannot read the pattern catalog that the findings cite by `pattern_id`.
- It references `agents-arz`, but the tracker prefix is now `sa-`.
- `catalog/skills/shatter-advise/scripts/test_discover_hotspots.py` is shipped in the payload.

### F7 shatter-agents CLAUDE.md does not import AGENTS.md (P2, AGENT)
`CLAUDE.md` (219 B) says "See AGENTS.md ... the content of interest is in AGENTS.md" with no `@AGENTS.md`.
Claude sessions there never load "Never edit `plugins/` by hand" or "Always run scripts/build-plugins".
bugshot does it correctly (`CLAUDE.md` = `@AGENTS.md`).

## 2. storystore: why it is dormant in shatter

Dormancy has four independent causes:

1. **The decision was made but never executed** (F8). str-qwua7.52 (adopt) has been open and
   unclaimed for 16 days.
2. **It would not work on shatter** (F9). Running the source-HEAD inventory on the audit worktree:
   ```
   python3 shared/inventory.py --repo-root <audit-wt>
   2753 surfaces: Counter({'test': 2664, 'heading': 73, 'skill': 15, 'bin': 1})
   cli-command: []
   languages: {'detected': ['go','javascript','rust','typescript'], 'extracted': ['javascript','typescript']}
   ```
   The only CLI extractor is `_CLI_COMMAND_RE = \.command\(\s*['"]...` (commander.js, inventory.py:139).
   Clap (Rust) and cobra/flag (Go) are not handled, so `stories-coverage`, whose headline default
   surface is `cli-command`, finds none of shatter's ~30 subcommands.
3. **The installed plugin is four months stale** (F10). The cache is `0.1.1` @ ca16aef (2026-05-10),
   128 commits behind HEAD. `plugin-version.json` was last bumped 2026-05-25 and has 65 commits since.
   `diff -rq` shows audit.py, coverage.py, inventory.py, impact_check.py and others differ, and
   `impact_trigger.py` is missing from the cache.
4. **Its tracker is write-blocked** (F11). `bd list` prints "refusing to auto-apply 21 pending schema
   migrations to a remote-backed database (v32 -> v53) ... Writes are blocked". Bugs found while
   adopting cannot be filed. The repo also has no AGENTS.md/CLAUDE.md, and four plan/design docs sit at
   the repo root (`2026-05-01-storystore-plan-*.md`).

## 3. bugshot

- bgs-3tq (CLI/TUI capture template), which str-qwua7.53 depends on, is **P3** in bugshot. The
  dependent issue is P2, and cross-repo dependencies are not modelled anywhere (F8).
- Every issue exists twice: 115 issues = 60 `bgs-*` + 55 `bugshot-*` mirrors, including 5 open
  chat-agent increments (`bugshot-47p/hx3/7g0/wyf/6zc`). shatter-agents has the same problem
  (`agents-xj6`/`sa-xj6` are both in_progress; `agents-ya6`/`sa-ya6`) (F13).
- bgs-3cz ("Published plugin bundle is 124 MB; 122 MB is node_modules") is closed. The fix added an
  opt-in `scripts/build-plugin --bundle-dir dist/bugshot`, but the bento marketplace still uses the
  repo root (`{"source":"github","repo":"ketang/bugshot"}`). The installed cache
  `~/.claude/plugins/cache/bento/bugshot/1.0.20` is 125 MB, with 122 MB of node_modules (dated
  06-17) plus `.beads/issues.jsonl`, `tests/` and `docs/` (F14; the node_modules origin is not fully
  traced).
- Positive: version-bump discipline works. The bugshot cache matches source except `__pycache__`, and
  duplicated per-skill Python copies are regenerated by `scripts/build-plugin` (md5 identical across
  4 copies).
- Minor: AGENTS.md "Project Structure" and "Documentation Sync Rules" cover only `skills/bugshot`, not
  vizline/vizdiff/wire-bugshot or `capture_runner.py`/`image_diff.py`. There is no README.

## 4. Global guidance

### F12 "Required Loads" never reach agents (P1, AGENT)
`~/dotfiles/docs/agent-guidance.md` lists Required Loads (branches-and-worktrees, read-before-designing,
verification, instruction-integrity "every session") and many conditional loads. None is
`@`-imported. The only thing that auto-loads is `codex/AGENTS.md`, via `@../codex/AGENTS.md`. A scan
of transcripts for tool calls whose `file_path` or `command` touches `agent-guidance/` or
`code-writing-guidance` found:

```
top-level sessions (~/.claude/projects/-home-ketan-project-shatter/*.jsonl): 0/87
subagent transcripts (*/subagents/*.jsonl):                                  1/166
```

The rules that address this audit's findings (wiring-and-consumption, drift-checks "check the
installed plugin/cache copy against the first-party source", failing-checks "a check documented as
required must be enforced") are effectively absent from agent context in this project.

### F13b Relative paths in the guidance index (P3, L3)
`agent-guidance.md` and `code-writing-guidance.md` use repo-relative paths (`docs/agent-guidance/verification.md`).
In a consumer repo they resolve against the consumer's `docs/`. Shatter has a `docs/` with no
`agent-guidance/`, and `instruction-integrity.md` tells the agent to treat an unresolvable referenced
rules file as a broken environment.

### F15 rtk still returns misleading content for `head -N` in compound commands (P2)
During this review, `ls -la ~/.claude/hooks/bento/ | head; head -5 ~/.claude/hooks/bento/require-worktree.sh; grep ...`
displayed `}` / `import json, os, sys` / `[164 more lines]` for the `head -5`. `/usr/bin/head -5` on the
same file prints `#!/usr/bin/env bash`, two comment lines and `set -euo pipefail`. The prefilter
(dotfiles #11, closed) skips redirects, find and git ref reads, but not exact-range reads displayed
to the agent.

### F16 Memory is still used as a tracker and is not retired (P2, AGENT)
- `project_shatter_gate_cache_and_bare_primary.md` says the primary checkout "has core.bare=true",
  which is false today. The workflow prompt for this very audit repeated the claim.
- `project_rtk_wrapper_corrupts_redirects.md` (08-24) predates the prefilter fix (09-07) and does not
  mention it.
- `project_pickpackit_*`, `project_kapow_*` and `project_zolem_*` (~28 KB) belong to other projects.
  `project_audit_2026_07_10_gate_state.md` is not in MEMORY.md.
- `codex/AGENTS.md` "Self-Improvement Loop" says "update memory with the lesson", with no rule to file
  the tooling bug in the owning repo, keep only a pointer, or retire the memory when the issue closes.
  shatter-agents AGENTS.md says the opposite ("prefer `bd remember`"). `bd memories` in shatter is
  empty.

### F17 Shatter's resource-etiquette rules sit inside the tool-managed rtk block (P2, AGENT)
`AGENTS.md:528` `<!-- headroom:rtk-instructions -->`, `:535` `## Shared-Machine Resource Etiquette (str-35vtk.5)`,
`:594` `<!-- /headroom:rtk-instructions -->`. `codex/AGENTS.md:10-12` says those markers are managed
per-repo by rtk's own tooling, so a regeneration would silently delete the heavyweight-slot and
parallelism rules. I did not run `rtk init` to confirm the overwrite. The same block's "always prefix
with rtk ... always safe" also appears in bugshot and shatter-agents AGENTS.md.

### F18 Small inaccuracies
`codex/AGENTS.md:23` says the require-worktree hook is "registered from `~/project/bento`". In fact
`~/.claude/hooks/bento/require-worktree.sh` symlinks to `~/.claude/plugins/cache/bento/bento/2.3.82/hooks/scripts/`.

## 5. Tracker hygiene seen across these repos
- sa-d1b "Add shatter-advise and shatter-gaps skills" is open, but both skills ship.
- bento-m4y5 "New bento SessionStart hook: agent-env doctor" is in_progress at P1, but the doctor ships.
- str-qwua7.1 is open, but the primary is repaired.
- shatter-agents commit fe46f8b is tracked by shatter issue str-391eh (cross-repo tracking).

## 6. Positives worth keeping
- The shatter-agents build discipline is sound: canonical `catalog/` feeds generated `plugins/`, with
  content-hash auto patch bump and CI `check-plugins-clean`.
- The sa-* bugs filed 2026-09-13 (sa-5cw, sa-oio, sa-c2q, sa-d8j) are exemplary: minimal fixtures,
  SHA-pinned evidence and read-only repro APIs.
- The bento doctor turned the 09-04 recommendations into working detection within ten days.
- The dotfiles issues filed from the last audit were all closed within 72 h, and the rtk prefilter
  has a byte-identity test suite.
- bugshot's generated-payload tests and version bumps keep its installed cache current.
- bgs-3tq itself is well specified: acceptance checks, env for colour, and normalize rules.
