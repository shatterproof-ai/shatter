# Shatter documentation quality & information-architecture review

Reviewer scope: quality, structure, audience fit, redundancy, navigability. Accuracy-vs-code is out of scope.
Date: 2026-09-03. Repo: /home/ketan/project/shatter (read-only).

## Inventory (tracked .md, excluding node_modules/target/.claude/.agent-plugins)

~100 files, ~21.7k lines, ~1.0 MB. Key sizes (lines / bytes):

| File | Lines | Bytes | Audience (declared or inferred) |
|---|---|---|---|
| SPEC.md | 1127 | 58 KB | users/contributors/auditors |
| PLAN.md | 1020 | 41 KB | historical roadmap (self-declared) |
| PROTOCOL.md | 608 | 18 KB | frontend implementors |
| AGENTS.md | 593 | 30 KB | agents (auto-loaded) |
| README.md | 408 | 16 KB | users |
| QUICKSTART.md | 169 | 5 KB | users |
| CONTRIBUTING.md | 152 | 5 KB | contributors |
| CLAUDE.md | 89 | 8.5 KB | agents (auto-loaded; @-imports AGENTS.md by reference) |
| LANGUAGE-EVALUATION.md | 150 | 14 KB | historical design rationale |
| PARITY.md (root) | 100 | 5 KB | ? (see P2-6) |
| ERRORS.md | 1 | 9 B | empty stub |
| docs/INDEX.md | 42 | 4.6 KB | everyone (doc map) |
| docs/PROJECT-LAYOUT.md | 335 | 10 KB | users+contributors |
| docs/execution-adapters.md | 878 | 26 KB | architects (self-declared "not current behavior") |
| docs/CI-INTEGRATION.md, DRIFT-PATROL.md, distribution.md, resource-parameters.md, GLOSSARY.md, hooks.md, performance-profiling.md, go-frontend-scope-limits.md | 44–335 | — | mixed |
| docs/plans/ (11 files) | 122–1264 | 6–40 KB | historical plans |
| docs/superpowers/plans (3) + specs (3) | 275–1534 | 14–54 KB | historical plans/specs |
| docs/specs/ (3), docs/research/ (1), docs/ideas/ (1), docs/validation/ (3), docs/perf/ (6), docs/audits/ (1) | — | — | mostly historical |
| audits/ (2 + evidence dir) | 230–282 | 14–17 KB | audit reports |
| shatter-go/CLAUDE.md | 324 | **55 KB** | agents (injected on demand) |
| shatter-ts/CLAUDE.md | 410 | 32 KB | agents |
| shatter-rust/CLAUDE.md | 263 | 22 KB | agents |
| shatter-core/CLAUDE.md | 88 | 6.7 KB | agents |
| shatter-cli/CLAUDE.md | 13 | 0.9 KB | agents |
| protocol/README.md, GOVERNANCE.md, PARITY.md, frontend-issue-template.md | 31–373 | — | frontend implementors |
| docker/README.md, perf/README.md, examples/go/*/README.md, tests/**/README.md | small | — | contributors |

`docs/stories/` does not exist. `docs/DRIFT-PATROL.md:42` lists a `docs-stories` drift check tied to `str-u394l.3` ("Stories coverage gate", status **open**). The storystore plugin is installed but dormant; there is no intent/story documentation anywhere, and the only reference to it is a not-implemented row in the drift-patrol table.

---

## Findings

### P1

**P1-1. AGENTS.md has an injected RTK block that swallowed a project section.**
`AGENTS.md:527` opens `<!-- headroom:rtk-instructions -->` and the closing marker is at `:593` (EOF). Between them sit the RTK "Key Commands"/"Rules" *and* the project-owned `## Shared-Machine Resource Etiquette (str-35vtk.5)` (`:534`). The headroom tool will treat that section as its own content on next re-injection, and readers see machine-wide gate governance nested under an "RTK (Rust Token Killer)" H1. Also the block introduces a second `# ` H1 mid-file.
Recommendation: move `## Shared-Machine Resource Etiquette` above the headroom marker (or into `docs/` and link it); keep third-party injected blocks at EOF.

**P1-2. Auto-loaded agent context is ~39 KB (≈10k tokens) per session and mixes durable rules with operational how-tos.**
CLAUDE.md (8.5 KB) + AGENTS.md (30.5 KB) are loaded every session. AGENTS.md content that is *not* needed every turn: Host Build Cache Setup (mold/sccache, `:42-85`), Kapow Refute Workflow (`:27-40`, project-external), full landing-the-plane git script, worktree seeding, merged-branch sweep script details, Beads sync-cadence rationale ("a recent audit found ~48%…"), the RTK cheat-sheet. Shatter also ships `.claude/skills/` (15 skills) and Bento skills that already encode much of the workflow (`bento:land-work`, `bento:beads-issue-flow`, `/pre-completion`), so the prose duplicates skill bodies.
Recommendation: keep AGENTS.md to rules + pointers (target ≤10 KB); move Host Build Cache, Kapow Refute, worktree seeding, resource etiquette to `docs/dev/…` and link; rely on skills for procedures. Run `bento:compress-docs`.

**P1-3. SPEC.md's own changelog rule is not being followed.**
Header (`SPEC.md:7`) says "Any CLI-visible change … should add a row to the changelog" and "Last updated: 2026-07-03". `git log -- SPEC.md` shows substantive edits on 2026-07-24 (wire `--concolic`/`--solver-timeout`), 2026-07-28 (invocation planner), 2026-08-26 (three-way exit code convention). The changelog (`§8`) has no rows after 2026-07-03. Two months of drift on the document that INDEX calls "authoritative" and that `/audit` compares against.
Recommendation: either add the missing rows and make the "Last updated" line part of the docs-smoke gate (fail if `git log -1` date > header date by > N days), or drop the manual changelog and rely on git history.

**P1-4. No convention for superseded/historical docs; INDEX.md ignores 27 orphan files.**
`docs/INDEX.md` links 17 documents and none of `docs/plans/`, `docs/specs/`, `docs/research/`, `docs/ideas/`, `docs/superpowers/`, `docs/validation/`, `docs/perf/`, `docs/audits/`, or root `audits/`. 27 markdown files are linked from nowhere (list below). Status markers are ad hoc: some plans say "Status: deferred" (`docs/plans/str-zwgc…`), some "Status: Draft/Approved" (`docs/superpowers/specs/*`), most have none (all four 2026-03-21 harness plans last touched 2026-03-28; `vscode-extension-design.md`, `str-kapl…`, `str-xmtw…` untouched since early March). PLAN.md and `docs/execution-adapters.md` do carry a clear "historical / not current behavior" banner — that is the right pattern but is applied inconsistently.
Orphans: ERRORS.md; audits/2026-02-28.md; audits/2026-05-21.md; docs/audits/2026-04-19-go-planner-parity.md; docs/ideas/factory-capture-exploration.md; docs/perf/{efficiency-plan,gate-overlap-audit,ws-a-cache-report,ws-cs-concurrency-report,ws-d-demo-cache-report}.md; docs/performance-profiling.md; docs/plans/{2026-03-21-semantic-harness-execution-plan,2026-04-15-hybrid-fuzzing,str-3ky9-external-dep-mocking,str-kapl-resilience-timeouts-memory,str-xmtw-symbolic-triage,telemetry-anonymous-usage-bad-args,vscode-extension-design}.md; docs/research/2026-04-go-stub-symbolic-returns.md; docs/specs/{2026-04-23-go-harness-retirement-checklist,concurrent-single-function-exploration}.md; docs/superpowers/plans/{2026-03-20-askama-html-templating,2026-03-21-stdin-harness-inputs,2026-03-23-stdin-harness-loop}.md; docs/superpowers/specs/{2026-03-17-mcdc-coverage-design,2026-05-23-str-7vs-llm-seed-oracle-design}.md; docs/validation/broad-run-corpus.md.
Recommendation: (a) adopt a required front-matter/banner: `Status: current | draft | approved | implemented (str-xxxx) | deferred | superseded-by <path>`; (b) add an "Archive / historical" section to INDEX.md listing each subdirectory with one-line purpose; (c) merge `docs/superpowers/{plans,specs}` into `docs/{plans,specs}` (two plan trees and two spec trees exist only because of the authoring tool); (d) merge root `audits/` into `docs/audits/`; (e) add an orphan check (script: every `docs/**/*.md` must be linked from INDEX.md or a parent README) to `task docs`.

### P2

**P2-1. Doc lint gates are opt-in and effectively not enforced in CI.**
`Taskfile.yml:352-378` (`task docs`) runs markdownlint-cli2, vale and lychee only `if command -v … ; else echo "[skip]"`. `.github/workflows/ci.yml` runs `task check` (which depends on `docs`) but does not install any of the three tools, so CI always prints `[skip]`. `.vale.ini` uses `BasedOnStyles = Vale` (spelling only) with a 40-word vocab; no house style (e.g. Microsoft/Google) or custom rules. Lychee excludes localhost only. Net: the only doc gate with teeth is `task docs-smoke` (README/QUICKSTART/SPEC/INDEX code fences), which is good but narrow.
Recommendation: install markdownlint-cli2 + lychee (offline mode) in ci.yml or make the skip a hard failure in CI (`CI=1` ⇒ fail if missing); decide whether vale is wanted — if yes, add a style; if no, delete `.vale.ini`/`styles/` to avoid false confidence.

**P2-2. Build/prerequisite instructions are maintained in three places with slightly different wording.**
README "Build from source" (≈45 lines), QUICKSTART "Build from source" (≈20 lines), CONTRIBUTING "Prerequisites" (≈25 lines) each list rustup/Node 22+/Go 1.24+/libclang/Z3/go-task/pyyaml+jsonschema with the same `apt install libclang-dev libz3-dev` line. Not contradictory today, but three copies is how drift starts. README also devotes ~35 lines to "Enabling Rust scans from a source build" and a `Makefile` wrapper convention that are contributor/integrator concerns, not user-entry concerns.
Recommendation: make CONTRIBUTING the single source for toolchain prerequisites; README/QUICKSTART keep only the binary install plus one link. Move "Enabling Rust scans from a source build" and "Build-tool wrappers" to CONTRIBUTING or docs/distribution.md.

**P2-3. CLAUDE.md contradicts PLAN.md's own status banner.**
`CLAUDE.md:5` "See `PLAN.md` for architecture" and `CLAUDE.md:87` "`PLAN.md` — architecture and implementation roadmap", while `PLAN.md:3` says "Status: historical roadmap — not current state". INDEX.md says "Roadmap — describes planned/in-progress work". An agent following CLAUDE.md is pointed at an out-of-date architecture description first. SPEC §1.2 has the current architecture sketch but is not referenced as such from CLAUDE.md.
Recommendation: change CLAUDE.md to "Architecture: SPEC.md §1; design rationale (historical): PLAN.md".

**P2-4. Tool-usage guidance in AGENTS.md conflicts with itself and with global instructions.**
`AGENTS.md:23-25` "Use the Grep, Glob, and Read tools — not shell grep/cat/sed/find/ls". `AGENTS.md:529-531` (RTK block) "When running shell commands, always prefix with `rtk`… `rtk grep <pattern>` `rtk read <file>`". The session also shadowed `grep` with an rtk wrapper. Three overlapping directives about the same action.
Recommendation: after P1-1, add one sentence reconciling them ("prefer dedicated tools; when Bash is unavoidable, prefix with rtk").

**P2-5. Crate CLAUDE.md files are very large and dense; shatter-go/CLAUDE.md is 55 KB in 324 lines (40 lines > 600 chars).**
These are injected whenever an agent reads a file in the crate, so a Go-frontend task pays ~14k tokens before starting; shatter-ts is 32 KB, shatter-rust 22 KB. Content is largely per-issue history ("### Vendor Resolution (str-nm5e)", "### go.work … (str-b66s)", "### Build Tag Activation (str-jl9r)") — changelog-shaped rather than rule-shaped. Contrast shatter-cli/CLAUDE.md (13 lines, purely orientation) which is the right shape.
Recommendation: split each crate CLAUDE.md into (a) ≤5 KB rules/contracts/key-files and (b) `docs/frontends/<lang>.md` reference for the history; wrap long lines.

**P2-6. Two "PARITY.md" files with different scopes; root one is unlinked and dated.**
Root `PARITY.md` ("Frontend Feature Parity Matrix", Last updated 2026-05-13, Y/N/P table) vs `protocol/PARITY.md` ("Frontend Parity Contract", classification + divergence register, backed by `parity-matrix.yaml`). Root PARITY.md is linked from nothing (only `protocol/PARITY.md` is linked, from DRIFT-PATROL). `task docs` lints root PARITY.md but INDEX doesn't list it. Likewise `LANGUAGE-EVALUATION.md` (historical core-language choice) and `ERRORS.md` (a one-line stub, `# Errors`) sit at repo root unlinked.
Recommendation: delete root PARITY.md or reduce it to a redirect; move LANGUAGE-EVALUATION.md to `docs/research/`; delete ERRORS.md or write it.

**P2-7. Audience bleed in user-facing docs.**
README "Executing Target Functions Safely" opens with "> **Breaking change (str-gg9v).**" and cites `str-02i70`; a public user has no tracker to resolve these. QUICKSTART §2 runs `./target/release/shatter …` first even for users who installed via `install.sh` in §1. INDEX.md audience column is good, but `docs/CI-INTEGRATION.md` is tagged "Contributors" while its title suggests users wiring Shatter into their CI (it is actually about *this repo's* CI). README has no link to PROTOCOL.md or GLOSSARY.md.
Recommendation: replace tracker IDs in README/SPEC with dated notes or a link to a public changelog; reorder QUICKSTART to assume `shatter` on PATH; rename `docs/CI-INTEGRATION.md` → `docs/dev/ci.md` (or clarify title) and reserve "CI integration" for the user-facing `docs/distribution.md`/`wire-shatter-ci` material.

**P2-8. Missing documents a user or contributor would look for.**
No CHANGELOG (SPEC §8 is the only changelog and it is stale — P1-3); no troubleshooting/FAQ (the docs contain many scattered "if X fails, do Y" notes: Z3 linker errors, Rust frontend not found, sandbox default-deny, sccache wedging); no per-frontend user guide (Go has `docs/go-frontend-scope-limits.md` but it is not in INDEX and only reachable from shatter-go/CLAUDE.md and two archived docs; TS and Rust have nothing); no consolidated config reference (`.shatter/config.yaml` schema is described as "see PROJECT-LAYOUT for the full schema" but PROJECT-LAYOUT §`.shatter/config.yaml` is 30 lines of prose, not a schema); no architecture doc that is both current and diagrammatic (SPEC §1.2 is a 10-line box sketch; PLAN's is historical). ADR-style rationale exists only as PLAN.md and LANGUAGE-EVALUATION.md.
Recommendation (issue-ready): `docs/troubleshooting.md` (collect existing notes); `docs/config-reference.md` generated or hand-written from the YAML schema; `docs/frontends/{typescript,go,rust}.md` user guides (promote go-frontend-scope-limits.md); `docs/architecture.md` (current, one diagram, linked from CLAUDE.md and INDEX); CHANGELOG.md or a generated release-notes page for continuous builds.

**P2-9. Audit follow-through is partial and audits are unlinked.**
2026-02-28 audit: all five filed issues (str-4ha, str-3mp, str-xve, str-zie, str-iam) are **closed**; README now documents core commands; skills are referenced from CLAUDE.md (P3-13 done); README lists shatter-rust. Good. 2026-05-21 audit: **no issues were filed at audit time** ("Issues Created: None" — blocked by an issue-completeness workflow). Later grep of `.beads/issues.jsonl` finds matching titles for panic_impl, external-audit explore, dry-run timeout, timeout phase, and Rust source discovery (README now documents auto-discovery of `shatter-rust/target/release`), but nothing for "JSON output consistency" or "mnemonic seed". Neither audit is linked from INDEX.md, and there is no "audit → issues → status" back-link, so verifying follow-through requires grepping the tracker.
Recommendation: add an `## Audits` section to INDEX.md; require each audit report to end with a filed-issue table (the Feb one does); file the two unfiled May drafts or record them as won't-do.

### P3

**P3-1. Terminology is consistent but GLOSSARY is narrow.** No occurrences of "behaviour", "front-end", "behaviour map"; "frontend" (64 files) and "behavior map" (12) are uniform. But GLOSSARY.md (56 lines) defines only strategy/frontier/nondeterminism/farming terms; it omits the vocabulary readers actually hit first: behavior map, behavioral spec vs behavior snapshot vs exploration report, equivalence class, harness, invocation planner, execution adapter, opaque type, staged pipeline (observe/analyze/solve/specify), external-audit mode, parity. Vale's vocab file (`styles/Vocab/Shatter/accept.txt`) and GLOSSARY are maintained separately.
Recommendation: extend GLOSSARY with the user-facing nouns; consider generating the Vale accept list from it.

**P3-2. Writing quality is generally good; a few structural nits.** README, QUICKSTART, CONTRIBUTING, PROJECT-LAYOUT and SPEC §1–3 are clear, use tables well, and state audience up front. Nits: SPEC has a stray second H1 in a fenced markdown sample (`SPEC.md:759 "# Specification: classifyNumber"`) which markdownlint would flag as MD025 if the fence language were not `markdown`; PLAN.md likewise embeds `# shatter.scope.yaml` etc. `docker/README.md:5` links `[str-umw3](../docs/)` to a directory. `docs/INDEX.md` calls itself "Documentation Map" but is not linked from CLAUDE.md/AGENTS.md (agents reach it only via CONTRIBUTING). README "Documentation" list and INDEX "How to Read These Docs" duplicate each other.

**P3-3. Taskfile (32 KB) is undocumented as a surface.** `task --list` is the only index of ~60 tasks; CLAUDE.md tables cover 9 tiers, hooks.md covers 3. No doc explains `check-static/check-unit` staging, `affected`, `walkthrough-cold`, `drift-patrol`, gate-wrapper slots except scattered across AGENTS.md and docs/perf/gate-budgets.md.
Recommendation: `docs/dev/tasks.md` generated from `task --list-all --json` or a short hand-written map.

**P3-4. Intent/story documentation is absent.** Nothing in the tree records user-level intent stories; the closest artefacts are the walkthrough (`demo/walkthrough.yaml`) and SPEC command sections. The drift-patrol table already reserves a `docs-stories` check (`str-u394l.3`, open). Either run `storystore:stories-init` and link `docs/stories/INDEX.md` from INDEX.md, or set `agent_env_doctor_skip_plugin=storystore` and remove the drift-patrol row so the docs stop promising a gate that does not exist.

---

## Summary table

| ID | Tag | One line |
|---|---|---|
| P1-1 | structure | RTK-injected block in AGENTS.md wraps a project section |
| P1-2 | load | ~39 KB auto-loaded agent docs; move how-tos out |
| P1-3 | staleness | SPEC changelog/Last-updated stale since 2026-07-03 despite header rule |
| P1-4 | navigation | 27 orphan docs, no archive convention, two plan trees, two spec trees, two audit dirs |
| P2-1 | lint | markdownlint/vale/lychee skip silently in CI |
| P2-2 | redundancy | prerequisites in README/QUICKSTART/CONTRIBUTING ×3 |
| P2-3 | contradiction | CLAUDE.md points to PLAN.md for architecture; PLAN says historical |
| P2-4 | contradiction | Grep/Read tools vs rtk-prefix directives |
| P2-5 | load | crate CLAUDE.md are changelog-shaped, 22–55 KB |
| P2-6 | navigation | root PARITY.md, LANGUAGE-EVALUATION.md, ERRORS.md unlinked/duplicative |
| P2-7 | audience | tracker IDs and build-path assumptions in user docs |
| P2-8 | missing | troubleshooting, config reference, frontend guides, architecture, changelog |
| P2-9 | audits | Feb audit fully actioned; May audit filed nothing at the time, 2 drafts still unfiled; audits unlinked |
| P3-1 | terminology | consistent; GLOSSARY too narrow |
| P3-2 | quality | prose good; minor H1/link nits |
| P3-3 | missing | Taskfile surface undocumented |
| P3-4 | stories | docs/stories absent; drift-patrol promises a gate that doesn't exist |
