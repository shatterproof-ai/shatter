# Audit 2026-09-22 — shatter / AGENT-level issue drafts

Selection: findings with `target_repo=shatter` and `level=AGENT`, verify verdict not refuted, dedupe relation `new`, `duplicate-closed-but-unfixed`, `partially-covered`, or `related` (treated as new-with-references, since no existing issue covers them). `duplicate-open` findings are listed below with the existing ID and not drafted.

Status: DRAFTS ONLY. Nothing has been filed. Before running `file.sh`, run `bento:issue-readiness-check` (fresh reviewer, draft file only) on each draft; revise any `ready: no`. Notes 17/30/31 append to existing issues.

## Drafts

| # | Title | Pri | Type | Action | Findings |
|---|---|---|---|---|---|
| 01 | [Epic: Audit 2026-09-22 findings (agent system / process)](01-epic-audit-2026-09-22-agent.md) | P1 | epic | epic | all AGENT-level shatter findings |
| 02 | [Fix scheduled Drift Patrol workflow (setup-go points at nonexistent root go.mod; 7/7 scheduled runs red)](02-drift-patrol-workflow-never-runs.md) | P1 | bug | new child of 01 | agent-repo-02, prior-01 (related: tests-ci-03) |
| 03 | [Surface persistently red GitHub workflows to agents (Build and Release 0/200, Perf CI 0/13, no issue)](03-workflow-health-signal.md) | P1 | task | new child of 01; blocked by 02 | agent-repo-03 (related L5: prior-02; bento side: tests-ci-03) |
| 04 | [Remove leaked fixture identity (Test <test@example.com>) from primary .git/config and add a repo-state check](04-repair-poisoned-git-identity.md) | P1 | bug | new child of 01 | agent-repo-01, prior-04 |
| 05 | [Replace removed `bd sync` in agent docs and refresh the frozen .beads/issues.jsonl snapshot (plus a freshness check)](05-bd-sync-removed-jsonl-stale.md) | P1 | bug | new child of 01 | prior-03, docs-16 |
| 06 | [Land the 2026-09-04 audit report on main and make /audit publish before filing](06-publish-audit-reports-and-audit-skill-landing.md) | P1 | task | new child of 01 | agent-repo-04, docs-04 |
| 07 | [Rewrite stale shatter agent memories that prescribe --no-verify/hooksPath bypass and state false repo facts](07-repair-stale-agent-memory.md) | P1 | task | new child of 01 | sessions-03, agent-repo-08, frontend-rust-10 (memory part) |
| 08 | [Decide and apply beads hook timeout policy (post-checkout ~300s per worktree/preview; str-qwua7.28 vs str-mpgg1 deadlock)](08-beads-hook-timeout-decision.md) | P1 | decision | new child of 01 | sessions-05, agent-repo-07 |
| 09 | [Gauntlet scan-failure checker has matched nothing since 2026-05-13; tests pin the dead format](09-gauntlet-scan-checker-dead.md) | P1 | bug | new child of 01 | artifacts-07 |
| 10 | [Tracker reconciliation: close resolved/obsolete/landed issues and add a landed-not-closed patrol check](10-tracker-reconciliation-sweep.md) | P2 | chore | new child of 01 | agent-repo-05, sessions-09, prior-09, protocol-parity-16, frontend-rust-10 (tracker part) |
| 11 | [Define P1, cap open P1s, and split/wave-order the stalled str-qwua7 audit epic](11-triage-policy-and-audit-epic-waves.md) | P2 | task | new child of 01 | prior-14, prior-25, agent-repo-18 |
| 12 | [Record the 2026-09-07 fixture-corruption incident, review recovery branches, and re-verify str-qwua7.14 on origin/main](12-fixture-corruption-incident-and-reverify.md) | P2 | task | new child of 01 | agent-repo-16 |
| 13 | [Make .claude/ and .codex/ agent config trackable despite the global gitignore](13-global-gitignore-hides-agent-config.md) | P2 | task | new child of 01 | agent-repo-10 |
| 14 | [Repair rotted repo skills (check-go/rust/ts bare commands, superseded protocol-sync, audit skill paths/steps)](14-repo-skills-rot.md) | P2 | task | new child of 01 | agent-repo-14 |
| 15 | [Rewrite frontend-parity skill workflow and extend frontend-issue-template parity checklist to Go/Rust builders](15-parity-guidance-skill-and-template.md) | P3 | task | new child of 01 | protocol-parity-17, protocol-parity-18 |
| 16 | [Apply the 2026-09-06 storystore/bugshot decisions to .agent-mode.local and clean doctor-flagged orphan worktrees](16-execute-env-doctor-decisions.md) | P2 | chore | new child of 01 | agent-repo-15, plugins-08 |
| 17 | [Note for str-qwua7.23: rtk-managed block also carries 'always safe' rtk claim contradicting memory/global rules](17-note-qwua7.23-rtk-block.md) | P2 | note | append notes → str-qwua7.23 | plugins-12 (related L3: agent-repo-09) |
| 18 | [Landing evidence must cover each changed language: verifier runs no TS/Go/rust-fe tests and hides output](18-verifier-per-language-evidence.md) | P2 | task | new child of 01 | tests-ci-06 (duplicate-open sibling: agent-repo-11 -> str-qwua7.55) |
| 19 | [Meta test: every scripts/test_*.py and demo/test_*.py must be run by a gate (≈220 unwired tests today)](19-wire-every-test-module.md) | P2 | task | new child of 01 | tests-ci-10, protocol-parity-05 |
| 20 | [Task `sources:` and affected-gates omit real inputs (parity matrix, runtime crate, rust-fe tests/, shatter-llm): add a coverage meta test](20-gate-sources-affected-completeness.md) | P2 | task | new child of 01 | protocol-parity-04, frontend-rust-07 |
| 21 | [Completion checklist and /pre-completion must require SPEC/QUICKSTART/changelog updates for CLI-visible changes](21-completion-checklist-spec-docs.md) | P2 | task | new child of 01 | docs-10 |
| 22 | [Planning rules in CLAUDE.md: plan/spec location + status banner, and check open tracker decisions before planning](22-planning-rules-location-and-open-decisions.md) | P2 | task | new child of 01 | docs-12, frontend-rust-09 (process part) |
| 23 | [Replace prose-only parallel-path parity with gates: engine_parity E2E, per-BranchType known-answer fixtures, caller-named closures](23-mechanical-parallel-parity-gates.md) | P2 | task | new child of 01 | core-22, frontend-ts-18 |
| 24 | [Producer/consumer contract suite for CLI artifacts: CLI-driven golden outputs, cross-format counts, consumer round-trips](24-cli-golden-output-contract-suite.md) | P2 | task | new child of 01; blocked by 09 | artifacts-18 |
| 25 | [Gate golangci-lint and gofmt in check-static (lint ungated; str-2tyfk and str-qwua7.32 closed on false 'lint passes')](25-gate-golangci-lint.md) | P2 | task | new child of 01 | frontend-go-05 |
| 26 | [Restore rustfmt cleanliness once and gate `cargo fmt --check` (tree drifted again after str-fr1v)](26-gate-rustfmt.md) | P2 | task | new child of 01 | sessions-11 |
| 27 | [Track downstream ≥90% coverage goals (kapow, zolem, pickpackit) in the tracker; stalled at 18-28% since 2026-07-07](27-downstream-coverage-goals-epic.md) | P2 | epic | new child of 01 | goals-09 |
| 28 | [Lint tracker IDs out of CLI help and file the unfiled 2026-09-04 UI findings](28-help-tracker-ids-lint-and-unfiled-ui-items.md) | P2 | task | new child of 01 | cli-ux-17 |
| 29 | [validate-parity: fail when a `tracked` divergence points at a closed or missing issue](29-divergence-tracking-issue-liveness.md) | P2 | task | new child of 01 | protocol-parity-12 |
| 30 | [Note for str-qwua7.37: Step 0 premise is wrong for Go package functions; wrong data path](30-note-qwua7.37-premise.md) | P3 | note | append notes → str-qwua7.37 | protocol-parity-14 |
| 31 | [Note for str-qwua7.43: bench_frontier_ranking.rs deepened the core→shatter-llm dev-dep cycle](31-note-qwua7.43-bench.md) | P2 | note | append notes → str-qwua7.43 | frontend-rust-09 |

## Skipped: duplicate-open (append evidence to the existing issue)

| Finding | Summary | Existing | Suggested note |
|---|---|---|---|
| protocol-parity-13 | Dangling divergence IDs / false preflight claims still unlanded | str-qwua7.34, str-qwua7.24 | Append shatter-go/CLAUDE.md:138 ('TS and Rust currently declare outcome only') to str-qwua7.24's stale-claim list. |
| protocol-parity-15 | Validator TS extraction still silently empty | str-qwua7.7 | Dedupe says duplicate-open but verify found str-qwua7.7 CLOSED (0655458b) with its 'empty extraction must fail' criterion unmet — maintainer should reopen .7 rather than file new. |
| docs-06 | Flag-level SPEC drift recurs; CLI-surface gate unimplemented | str-wurp | Add the changelog-row rule and reverse (SPEC→clap) flag check to str-wurp acceptance. |
| docs-17 | Doc lint tools skip in landing gate | str-qwua7.46 | No new content. |
| agent-repo-11 | Verifier false 'same gates as CI' claim, output discarded, no executed flag | str-qwua7.55, str-qwua7.2 | Adoption of bento-rdtn.4/.6 folded into draft 18 and a note on .55. |
| prior-12 | Approved tracker-sweep decisions unexecuted | str-qwua7.62 | Tracker-only task path covered in draft 10 note; execute .62. |
| prior-21 | Drift-patrol PENDING checks unimplemented; docs/stories absent | str-qwua7.52, str-u394l.3, str-wurp | New idea: PENDING slot older than 60 days turns FAIL — append to str-u394l.3. |

## Out of this selection (other trackers or levels)

| Findings | Target | Note |
|---|---|---|
| cli-ux-20 | bento | Shared scratchpad across parallel audit subagents; verifier says fix belongs in shatter audit skill/workflow prompt — consider retargeting to shatter. |
| tests-ci-03 | bento | Red scheduled workflows invisible; shatter-side workflow fixes covered by drafts 02-03. |
| core-23, prior-06, prior-10, prior-11, prior-13, agent-repo-17, sessions-02/07/10/15/17 | bento | bento-side landing/guard/doctor items. |
| sessions-01/06/12/13/14/16, plugins-04/06/13/15/19, protocol-parity-21 | dotfiles | global guidance/settings. |
| artifacts-11, plugins-03, goals-11 | shatter-agents | plugin skills vs CLI. |
| goals-10 | other | effectiveness harness repos. |
| agent-repo-06/09/12/13/19, prior-02 | shatter (non-AGENT level) | Closely related L2/L3/L5 items; drafted under the product epic, referenced here for context. |

## Secondary notes to append (from drafts)

- str-qwua7.51: real root cause is leaked repo-local `[user]` (draft 04).
- str-qwua7.28: correct stale setup-hooks.sh:41 fact; link decision draft 08.
- str-qwua7.62: link reconciliation draft 10.
- str-qwua7.55: link per-language evidence draft 18.
- str-qwua7.43: bench_frontier_ranking scope (draft 31; also referenced from 22).

## Cross-set duplicates (added by the completeness review)

Before running `file.sh`, read §15.1 of `audits/2026-09-22.md`. Drafts in this set that duplicate or overlap a draft in another set:
- 25 = `shatter-code/57` (golangci-lint gate; keep one).
- 28 overlaps `shatter-docs-ui/08` (merge).
- 17 (note on str-qwua7.23) overlaps `shatter-docs-ui/30`; post one combined note.
- 20 overlaps `shatter-code/03` and `shatter-code/04`.
- 03 and 06 are P1 here but P2 in the report's §14/§15; reconcile.
