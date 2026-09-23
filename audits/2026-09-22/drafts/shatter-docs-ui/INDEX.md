# Audit 2026-09-22: shatter issue drafts for L2, L3 and L6 (docs and UI)

These drafts were selected with: `target_repo == shatter`, level in {L2, L3, L6}, verify verdict not refuted, and dedupe relation in {new, duplicate-closed-but-unfixed, partially-covered}. Closely related findings are grouped into single drafts. Nothing has been filed yet: review this file, then run `APPLY=1 ./file.sh` (the default is a dry run).

Tracker layout: `file.sh` finds or creates **"Epic: Audit 2026-09-22 findings"**, which other audit batches share. It then creates the child epic **"Epic: Audit 2026-09-22 — docs accuracy and CLI/report UX"** and files the new issues below under that child epic. Notes are added as comments on the existing issues they belong to.

Readiness: each draft was written to the bento `issue-readiness-check` standard (problem, evidence with file:line, current code facts, acceptance criteria, suggested approach, scope, priority/type/labels/relations). The skill's fresh-reviewer pass could **not** run here, because this subagent has no subagent/Task tool, so every draft has only a `review_mode: local-fallback` self-check. Run a fresh reviewer on each draft before filing (priority: 01, 02, 05, 22). Every body carries its evidence inline because the audit report is on the unpushed branch `audit-2026-09-22`.

## New issues (27)

| # | Title | Pri | Type | Source findings | Dedupe / relation |
|---|---|---|---|---|---|
| 01 | SHATTER_SANDBOX_BACKEND disables the host-write guard for TS/Rust; docs recommend it | P1 | bug | docs-01 | dup-closed-unfixed str-qwua7.8 (docs half); code half new |
| 02 | Explore markdown under-reports discovered paths | P1 | bug | goals-03, prior-19 | new; same root cause as L5 core-02/cli-ux-15/artifacts-03 (merge or link if a core issue is filed) |
| 04 | SPEC changelog claims §2.8/§2.9/§3.6 updates that never happened | P2 | bug | docs-05 | dup-closed-unfixed str-qwua7.8 |
| 05 | Output-artifact schemas and SPEC §5 producer/consumer table; rewrite §5 samples | P2 | task | artifacts-10, docs-03, docs-08 | new |
| 06 | SPEC §6 stale; checkpoint and artifacts split across two dirs | P2 | bug | docs-07 | new; link L5 artifacts-04 |
| 07 | Execution-only flags still shown in help (`help <cmd>`, 9 commands) | P2 | bug | prior-07, docs-22 | dup-closed-unfixed str-qwua7.15; design half is L4 cli-ux-05 |
| 08 | Remove tracker IDs from --help, SPEC and README | P3 | task | docs-18 | partially covered by str-qwua7.45 |
| 09 | Scan progress printed after scan ends; JSON mixed into stderr; `run` silent | P2 | bug | cli-ux-06 | dup-closed-unfixed str-7pkp.5 |
| 10 | Rust hint fails on undocumented SHATTER_RUNTIME_PATH; doctor reports all green | P2 | bug | cli-ux-10 | partially covered by str-qwua7.40/.20.2/.60/.13 |
| 11 | Error outcomes render inconsistently per language; Rust shows truncated serde JSON | P2 | bug | cli-ux-11 | new |
| 12 | Default markdown drops branches, discovery method and stop reason | P3 | feature | cli-ux-14 | new |
| 13 | Minor output defects (format text, stray header, lang 'any', green errors) | P3 | bug | artifacts-16 | new |
| 14 | `--analyze-only` refused without a sandbox and shows no types; error/help polish | P3 | bug | goals-18, cli-ux-19 | cli-ux-19 partially covered by str-qwua7.12/.33 |
| 15 | Spec markdown invariants have blank subjects and duplicate lines | P2 | bug | artifacts-05 | partially covered by str-qwua7.61 |
| 16 | Spec/properties YAML uses `!AllEqual` tags that standard loaders reject | P2 | bug | artifacts-14 | new |
| 17 | Scan report: 100% HTML headline, absolute paths, zero rows, undefined Interesting Inputs | P2 | bug | artifacts-15 (+ artifacts-13 folded in) | new |
| 18 | Source-bucket classifier treats a `fixture` package as fixture_sample | P3 | bug | goals-16 | dup-closed-unfixed str-9awj (same class) |
| 19 | Reports embed raw NUL/ESC bytes | P3 | bug | goals-17 | new |
| 20 | "N/N branches" counts sites, not sides | P2 | bug | frontend-go-12 (metric part), core-18 | go-12 partially covered by str-qwua7.39 (its init part is in note 03) |
| 21 | CLAUDE.md tier table overstates coverage; check-fast stale; e2e runs twice | P3 | task | gates-07, tests-ci-17 | new |
| 22 | Correct stale facts in the four crate CLAUDE.md files | P2 | task | core-21, frontend-ts-15, frontend-go-09, frontend-rust-08 | partially covered by str-qwua7.24/.25/.34 (new issue for the fact fixes) |
| 24 | Protocol schemas reject real frontend output (shl/shr/bit_clear) | P2 | bug | protocol-parity-03 | partially covered by str-2fjn |
| 25 | protocol/PARITY.md hand-mirrors the matrix and is stale | P2 | bug | protocol-parity-06 | partially covered by str-qwua7.24 |
| 26 | protocol.rs doc comments misstate `outcome` emitters and `runtime_crypto_boundaries` | P3 | bug | protocol-parity-10 | new |
| 27 | Go never emits connection_failures, so LiveFirst never fires; no divergence recorded | P2 | bug | protocol-parity-11 | partially covered by str-2fjn notes |
| 28 | Move protocol/ test doubles; delete dead delayed-execute-frontend.sh | P3 | chore | protocol-parity-20 | new |
| 29 | Extend docs-smoke to 4 more docs and PROTOCOL.md | P3 | task | docs-19 | new |

## Notes to append to existing issues (3)

| # | Target | Content | Source findings |
|---|---|---|---|
| 03 | str-qwua7.39 | First-run `scan --format json` stdout is not JSON; widen to every JSON command; empty init path; **raise to P1** (`BUMP_PRIORITY=1`) | cli-ux-04, frontend-go-12 (init part) |
| 23 | str-rf2v | The TS analyzer ignores data flow; the parity matrix wrongly says TS analyze produces ite; fourth walker copy in executor.ts | frontend-ts-03 |
| 30 | str-qwua7.23 | Etiquette section inside the rtk block; landing prose vs land.py; 38 merged plus 4 contaminated remote branches | agent-repo-09, agent-repo-12 |

## Skipped: duplicate-open (existing issue already covers it)

| Finding | Existing issue | Suggested action |
|---|---|---|
| cli-ux-21, artifacts-17 (`--spec-json` stdout is markdown then JSON) | str-qwua7.11 (P1) | comment: also point spec-diff/compare help at `--spec-out` |
| docs-11, goals-19 (no docs/stories) | str-qwua7.52, str-u394l.3 | comment: seed story list (baseline→change→detect journeys) |
| docs-13 (doc IA debt, about 40 orphans) | str-qwua7.44, str-qwua7.45 | none |
| docs-14 (config reference gap) | str-qwua7.21.1 | comment: generate from schemars; fix README/PROJECT-LAYOUT pointer loop |
| docs-15 (implicit init undocumented, writes to stdout) | str-qwua7.58, str-qwua7.39 | covered by note 03 |
| docs-20 (behavior-class terminology) | str-qwua7.57 | none |
| docs-21 (decided removals still documented) | str-qwua7.61, str-qwua7.59 | consider splitting out the doc half |
| docs-24 (crate CLAUDE.md sizes, .js refs) | str-qwua7.25, str-qwua7.23 | .js refs also fixed by draft 22 |
| agent-repo-11 (verifier false claim, discards output) | str-qwua7.55, str-qwua7.2 | none |
| agent-repo-13 (frontend-parity skill contradicts the matrix) | str-qwua7.24 | none |

## Not drafted: dedupe relation "related", excluded by the selection rule (consider a triage pass)

Several of these are real user-visible bugs that no issue tracks yet. File them in a follow-up batch or fold them into the drafts noted.
- **cli-ux-03** (P2): `-o FILE --stdout` prints the explore report twice; `-q -o` leaks the header. Related str-zt4v/str-6c6p. *Worth filing.*
- **cli-ux-13** (P2): run report opens with an unexplained "degraded" verdict before its H1; explore, scan and run use different coverage metrics.
- **artifacts-13** (P2): HTML "Paths"/"Paths Found" shows branches_covered. Folded into draft 17.
- **core-18** (P3): branch counter counts sites. Folded into draft 20.
- **frontend-ts-08** (P2): shatter-ts CLAUDE.md promises `not_supported` for planner probes; TS returns `invalid_request`. Candidate acceptance item for str-mhinv.3.
- **frontend-go-08** (P2): `--build-timeout`/SHATTER_BUILD_TIMEOUT and SHATTER_HARNESS_RELEASE ignored by Go; `go build` has no timeout. *Worth filing* (the timeout is apparently lost since str-9smo).
- **frontend-go-13** (P3): cgo "detect and refuse" covers signatures only.
- **frontend-rust-18** (P3): no user guide for the LLM seed oracle; candidate child of str-qwua7.21.
- **protocol-parity-07** (P2): GOVERNANCE.md omits the matrix, codegen and validate-parity, and names two authorities.
- **protocol-parity-09** (P2): PROTOCOL.md execute section documents about half the fields.
- **tests-ci-14** (P3): formal-methods-policy prescribes cargo-fuzz; none exists (str-df9g possibly closed-unfixed).
- **docs-23** (P3): DRIFT-PATROL.md table omits the tracker-server check.
- **agent-repo-06** (P2): AGENTS.md mandates `bd sync` (absent in bd 1.1.0); tracked JSONL 15 days stale. Likely drafted by the AGENT batch (prior-03).
- **agent-repo-19** (P3): `.claude/swarm-config.md` is not read by bento swarm; str-35vtk.9 scope is stale.

## Out of scope for this batch (target_repo is not shatter)
- docs-09 (shatter-agents; wrapper convention runs bare `shatter`; partly covered by sa-oio) and goals-11 (shatter-agents shatter-diff skill). The README half of docs-09 is shatter-side; str-dakf3 covers the Makefile example.

## Cross-set duplicates (added by the completeness review)

Before running `file.sh`, read §15.1 of `audits/2026-09-22.md`. Drafts in this set that duplicate or overlap a draft in another set:
- 02 = `shatter-code/12` (same root cause; drop 02 or add its evidence to code/12).
- 07 overlaps `shatter-code/28` (merge).
- 08 overlaps `shatter-agent/28` (merge; pick one priority).
- 21 overlaps `shatter-code/74` (e2e duplication item).
- Notes 23 and 30 overlap `shatter-code/45` and `shatter-agent/17` (same target issues).
- The "related" findings listed under "Not drafted" still need drafts; six are P2 (cli-ux-03, cli-ux-13, frontend-ts-08, frontend-go-08, protocol-parity-07, protocol-parity-09).
