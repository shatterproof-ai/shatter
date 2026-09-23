# Audit 2026-09-22: other first-party issue drafts (not filed)

Scope: findings whose `target_repo` is dotfiles, shatter-agents, storystore, bugshot or other. The findings had to have a verify verdict other than refuted, and a dedupe relation of new, duplicate-closed-but-unfixed or partially-covered. Findings marked `related` were treated as new when the dedupe note said no existing issue covers them; each such draft says so. Findings targeted at bento or shatter are drafted elsewhere.

Filing: `./file.sh` does a dry run; `./file.sh --apply` files. It is not run by the drafting agent. **Readiness:** every draft was prechecked in `local-fallback` mode, because this runtime exposed no subagent tool. Run bento:issue-readiness-check with a fresh reviewer before `--apply`.

## dotfiles (GitHub Issues, `ketang/dotfiles`)

| # | Draft | Title | Pri | Type | Findings | Relation |
|---|---|---|---|---|---|---|
| 00 | [00-dotfiles-epic.md](00-dotfiles-epic.md) | Epic: Audit 2026-09-22 findings (global agent guidance and hooks) | P1 | epic | - | - |
| 01 | [01-dotfiles-guidance-loading.md](01-dotfiles-guidance-loading.md) | Make the global Required-Loads guidance actually load | P1 | bug | plugins-06, plugins-19 | related to #9/#10, treated as new |
| 02 | [02-dotfiles-background-wait.md](02-dotfiles-background-wait.md) | "Waiting for background work" rule and no-op-poll guard hook | P2 | enhancement | sessions-01 | partially-covered (str-qwua7.26) |
| 03 | [03-dotfiles-bypass-framing.md](03-dotfiles-bypass-framing.md) | Never mark a bypass as the recommended option | P2 | docs | sessions-06 | new |
| 04 | [04-dotfiles-plugin-autoupdate.md](04-dotfiles-plugin-autoupdate.md) | Plugin autoUpdate for first-party marketplaces and stale-plugin detection | P2 | bug | plugins-04 | new |
| 05 | [05-dotfiles-rtk-head-range.md](05-dotfiles-rtk-head-range.md) | rtk still shows summarized `head -N` in compound commands | P2 | bug | plugins-13 | closed-but-unfixed #11, new follow-up |
| 06 | [06-dotfiles-memory-lifecycle.md](06-dotfiles-memory-lifecycle.md) | Memory lifecycle rule (tracker first, memory as pointer, retire on close) | P2 | docs | plugins-15 (+ sessions-03 context) | partially-covered (#5-#7) |
| 07 | [07-dotfiles-validator-canary.md](07-dotfiles-validator-canary.md) | Validators must fail on empty extraction and carry a canary test | P3 | docs | protocol-parity-21 | new |
| 08 | [08-dotfiles-hooks-dotfiles-env.md](08-dotfiles-hooks-dotfiles-env.md) | Global hooks reference unset `$DOTFILES` | P3 | bug | sessions-14 | related to bento-m4y5, treated as new |
| 09 | [09-dotfiles-falsification-probe.md](09-dotfiles-falsification-probe.md) | Experiment plans start with a falsification probe | P3 | docs | sessions-12 | new |
| 10 | [10-dotfiles-blocked-escalation.md](10-dotfiles-blocked-escalation.md) | Escalation when blocked on the user or on a classifier denial | P3 | docs | sessions-13 | new |
| 11 | [11-dotfiles-tool-precedence-harness.md](11-dotfiles-tool-precedence-harness.md) | Reconcile Read/Grep-first with the harness bypass-mode guidance | P3 | docs | sessions-16 | partially-covered (#10, #11) |

## shatter-agents (bd, `/home/ketan/project/shatter-agents`, prefix `sa-`)

| # | Draft | Title | Pri | Type | Findings | Relation |
|---|---|---|---|---|---|---|
| 20 | [20-shatter-agents-epic.md](20-shatter-agents-epic.md) | Epic: Audit 2026-09-22 findings (shatter-agents plugin) | P1 | epic | - | - |
| 21 | [21-sa-shatter-diff-nonexistent.md](21-sa-shatter-diff-nonexistent.md) | shatter-diff skill documents a nonexistent command; its hook blocks commits | P1 | bug | plugins-01, goals-11, artifacts-11 (merged) | closed-but-unfixed sa-tyb, new issue plus note on sa-tyb |
| 22 | [22-sa-recipe-unimplemented.md](22-sa-recipe-unimplemented.md) | Recipe/stubs documented but not implemented | P1 | bug | plugins-02 | closed-but-unfixed sa-yyt, new issue plus note on sa-yyt |
| 23 | [23-sa-cli-contract-test.md](23-sa-cli-contract-test.md) | Skill vs pinned CLI contract test; requires/status metadata | P2 | task | plugins-03 | partially-covered (str-wurp, str-u394l.4) |
| 24 | [24-sa-delegate-to-engine.md](24-sa-delegate-to-engine.md) | Delegate discovery/doctor to `shatter list-targets`/`shatter doctor` | P2 | task | plugins-10 | partially-covered (sa-d8j, sa-c2q, sa-oio) |
| 25 | [25-sa-note-sa-oio.md](25-sa-note-sa-oio.md) | **Note to append to sa-oio**: default-deny policy, subcommand validation | (P1 existing) | note | docs-09 | partially-covered (sa-oio) |
| 26 | [26-sa-wire-shatter-ci.md](26-sa-wire-shatter-ci.md) | wire-shatter-ci workflow needs plugin-internal script; installer from main | P2 | bug | plugins-11 | new |
| 27 | [27-sa-advise-taxonomy-payload.md](27-sa-advise-taxonomy-payload.md) | Taxonomy spec not shipped; stale ID; test file in payload | P2 | bug | plugins-14 | new |
| 28 | [28-sa-claude-md-import.md](28-sa-claude-md-import.md) | CLAUDE.md does not import AGENTS.md | P2 | bug | plugins-17 | new |
| 29 | [29-sa-tracker-mirrors.md](29-sa-tracker-mirrors.md) | Close agents-* mirrors; close shipped sa-d1b | P2 | chore | plugins-16 (sa part) | new |

## storystore (bd, `/home/ketan/project/storystore`). Writes are blocked until the schema migration runs.

| # | Draft | Title | Pri | Type | Findings | Relation |
|---|---|---|---|---|---|---|
| 30 | [30-storystore-epic.md](30-storystore-epic.md) | Epic: Audit 2026-09-22 findings (storystore) | P2 | epic | - | - |
| 31 | [31-ss-migration-agents-md.md](31-ss-migration-agents-md.md) | Unblock tracker (v32->v53 migration); add AGENTS.md/CLAUDE.md | P2 | chore | plugins-09 | new |
| 32 | [32-ss-clap-cobra-extractors.md](32-ss-clap-cobra-extractors.md) | Rust clap and Go cobra CLI extractors; warn on unextracted languages | P2 | feature | plugins-05 | partially-covered (ss-yoa closed), new issue plus note on ss-yoa |
| 33 | [33-ss-version-bump.md](33-ss-version-bump.md) | Automatic version bumps; cache 128 commits behind | P2 | chore | plugins-07 | new |

## bugshot (bd, `/home/ketan/project/bugshot`, prefix `bgs-`)

| # | Draft | Title | Pri | Type | Findings | Relation |
|---|---|---|---|---|---|---|
| 40 | [40-bugshot-epic.md](40-bugshot-epic.md) | Epic: Audit 2026-09-22 findings (bugshot) | P2 | epic | - | - |
| 41 | [41-bgs-dedupe-mirrors.md](41-bgs-dedupe-mirrors.md) | Close 55 bugshot-* mirror duplicates | P2 | chore | plugins-16 | new |
| 42 | [42-bgs-installed-cache-bloat.md](42-bgs-installed-cache-bloat.md) | Investigate the 125 MB installed cache despite the bgs-3cz fix | P3 | task | plugins-18 | closed-but-unfixed bgs-3cz, new investigation plus note on bgs-3cz |
| 43 | [43-bgs-agents-md-structure.md](43-bgs-agents-md-structure.md) | AGENTS.md structure and sync rules for all skills; README | P3 | chore | plugins-20 | partially-covered (bgs-3tq) |

Also in file.sh: raise **bgs-3tq** to P2 with a note, because shatter str-qwua7.53 (P2) depends on it. This comes from finding plugins-08, which targets shatter.

## other

| # | Draft | Title | Pri | Type | Findings | Relation |
|---|---|---|---|---|---|---|
| 50 | [50-other-effectiveness-benchmark.md](50-other-effectiveness-benchmark.md) | Minimal effectiveness benchmark; retire or fix holdout | P2 | task | goals-10 | new. Filed in the **shatter** tracker because shatter-effectiveness has no tracker and holdout's bd is empty |

## Notes to append to existing issues (done by file.sh)

- sa-oio: full note in draft 25.
- sa-tyb: points to draft 21's new issue.
- sa-yyt: points to draft 22's new issue.
- ss-yoa: points to draft 32's new issue (after the storystore migration).
- bgs-3cz: points to draft 42's new issue.
- bgs-3tq: priority raised to P2, with a note.

## Skipped as duplicate-open

None of the in-scope findings was duplicate-open. Related findings were merged into one draft instead of being filed separately: goals-11 and artifacts-11 into draft 21.

## Out of scope for this batch

Findings with target_repo bento were drafted separately: core-23, cli-ux-20 (the verifier says it belongs in the audit workflow, not bento), tests-ci-03, agent-repo-17, sessions-02/07/10/15/17, bento-01..18, prior-06/10/11/13. The dotfiles-adjacent recommendations inside shatter-targeted findings plugins-12 (rtk template "always safe" text) and plugins-08 (skip key) remain in the shatter drafts.
