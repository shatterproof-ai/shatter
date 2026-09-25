# Bundle: shatter-concolic-and-engine-design

- **Bucket:** shatter-concolic-and-engine-design
- **Repo / tracker:** shatter, bd in /home/ketan/project/shatter (prefix str)
- **Parent epic:** Epic: Audit 2026-09-22 findings
- **Theme:** Measure concolic before positioning it (D3), engine parity, path/budget semantics, dead code, and effectiveness measurement.
- **Status:** drafts only; nothing filed (D6). Revised after the Codex cross-check (REVISION.md).
- **Evidence base:** audit worktree `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22` at HEAD 56c86168. Line numbers were re-verified against that tree on 2026-09-23.

## Maintainer decisions (2026-09-23)

- **D1 Releases:** keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix. Fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64); do not drop them. Release work closes only with a green release-run URL.
- **D2 shatter diff:** retire snapshot `shatter diff` and the unused Snapshot writer path. spec-diff is the regression tool. Update SPEC/README/QUICKSTART. The `diff` name becomes free; str-81xiw decides whether to use it. Correct the shatter-agents plugin's `shatter diff --staged` docs.
- **D3 Concolic positioning:** measure first.
  - P1: a controlled default-vs-concolic benchmark, reported per release.
  - P1: fix concolic early termination.
  - A follow-up decision issue, blocked by both, re-decides "concolic-first" positioning. No doc softening now.
- **D4 Beads hook stall:** retire the JSONL import in shatter and move tracker sync to a Dolt remote.
  - The first step verifies whether the stale-JSONL import has clobbered newer DB state.
  - AGENTS.md drops `bd sync`, and str-qwua7.28 is superseded.
  - bento beads-issue-flow gets matching guidance.
  - No BEADS_HOOK_TIMEOUT or hook-bypass guidance.
- **D5 Git identity:** the leaked [user] section was already removed. Add:
  - a `.mailmap` (test@example.com -> Ketan Gangatirkar <33678+ketang@users.noreply.github.com>, no history rewrite);
  - a git-state check (note on str-qwua7.1);
  - a `.git/config` before/after snapshot in `test_git_fixture_isolation.py`.
- **D6 Filing:** one filer script, run by the maintainer after reconciliation and the Codex cross-check. No agent files anything.

Decisions touching this bucket: **D3** (drafts 01-03, 11-15: measure first; P1 benchmark and P1 early-termination work; no doc softening). **D2** is referenced by 04 (spec-diff is the regression tool). D6 applies to all.

Revision 2026-09-23: revised after the Codex cross-check; see REVISION.md. Drafts 02, 04, 05, 06 and 07 were split; 11-21 are new.

## Contents

| # | Slug | Kind | Priority | Blocked by | Title |
|---|---|---|---|---|---|
| 01 | concolic-vs-default-benchmark | new | P1 | [explore-stop-reason-accounting, explore-budget-semantics, concolic-fuzz-rng-unseeded] | Add a controlled default-vs-concolic coverage benchmark (fixed seeds, isolated caches, equal execution budgets, examples corpus + one downstream subset) |
| 02 | concolic-early-termination | new | P1 | [explore-stop-reason-accounting] | Diagnose why --concolic explore runs end after 21-35 executions on most hard TS functions (diagnosis only; fix is concolic-early-termination-fix) |
| 03 | concolic-positioning-decision | new | P2 | [concolic-benchmark-postfix-run] | Decision: keep or revise the 'concolic-first' product positioning in README/SPEC, using the post-fix default-vs-concolic benchmark results |
| 04 | effectiveness-benchmark-holdout | new | P2 | [] | Deliver a minimal bug-finding effectiveness benchmark with a defined scoring oracle (seeded faults, detection rule, denominator, false-positive control) |
| 05 | engine-path-identity-budget-config | new | P2 | [] | Random explorer and concolic orchestrator use different path identities (bucketed path_hash vs raw hash_branch_path); define one canonical identity and test it on shared traces |
| 06 | engine-parity-e2e | new | P2 | [] | Add an engine_parity E2E suite that runs fixtures under random and concolic through the production pipeline, with strict expected-divergence markers |
| 07 | core-dead-code-removal | new | P2 | [] | Remove ~3,400 lines of production-dead shatter-core code (recursive, array_mutation, reporter+clustering, sequential scan(), shrink_witness) |
| 08 | stable-hash-persisted-keys | new | P3 | [] | std DefaultHasher keys persisted or promised-reproducible values (scan --seed sampling, Rust harness caches, external-audit cache dir, run scope_hash, ExecutionRecord.input_hash); use a specified hash |
| 09 | qwua7-6-function-length-ratchet | note-to-existing | P3 | [] | Note on str-qwua7.6: explore_with_oracle grew 1,307 -> 1,372 lines; add a function-length ratchet |
| 10 | qwua7-43-bench-dev-dep-cycle | note-to-existing | P2 | [] | Note on str-qwua7.43: bench_frontier_ranking.rs deepened the core -> shatter-llm dev-dependency cycle |
| 11 | explore-stop-reason-accounting | new | P1 | [] | explore artifacts always report stop_reason worklist_exhausted and solver_guided_inputs 0: ExploreResultAccumulator drops both fields |
| 12 | concolic-early-termination-fix | new | P1 | [concolic-early-termination] | Fix the concolic early-termination defect identified by concolic-early-termination, with a known-answer E2E test under a bounded budget |
| 13 | explore-budget-semantics | new | P1 | [] | --max-iterations means a different budget per command and engine (concolic max_executions 1x in explore, 5x in scan/observe); give it one meaning and an explicit execution budget |
| 14 | concolic-fuzz-rng-unseeded | new | P1 | [] | Concolic plateau fuzz phase uses StdRng::from_os_rng and ignores --seed, so seeded concolic runs are not reproducible |
| 15 | concolic-benchmark-postfix-run | new | P1 | [concolic-vs-default-benchmark, concolic-early-termination-fix] | Run the default-vs-concolic benchmark after the early-termination fix and commit the results the positioning decision uses |
| 16 | holdout-disposition | new | P3 | [effectiveness-benchmark-holdout] | holdout reports impossible totals (294/294 branches, 401 fingerprint skips counted as errors) and has not run since April; archive or fix it |
| 17 | effectiveness-repo-tracker-backlog | new | P3 | [effectiveness-benchmark-holdout] | If the effectiveness benchmark lives in shatter-effectiveness: initialize a tracker there and file the remaining plan tasks |
| 18 | underscore-binding-lint | new | P3 | [] | Lint _-prefixed parameters and let bindings in shatter-core/shatter-cli non-test code (they hid the concolic mock-variation regression) |
| 19 | pipeline-close-reason-rule | new | P3 | [] | CLAUDE.md completion checklist: pipeline fixes must name the production call site and the pipeline-level test; test workarounds in prose must be filed |
| 20 | core-reachability-gate | new | P3 | [core-dead-code-removal] | Add a production-reachability check for shatter-core pub items (defined roots and graph traversal), wired into check-static |
| 21 | qwua7-6-2-scan-observe-config-literals | note-to-existing | P1 | [] | Note on str-qwua7.6.2: scan and observe also hand-build orchestrator::ExploreConfig, and the three literals already diverge |

---
slug: concolic-vs-default-benchmark
kind: new
title: "Add a controlled default-vs-concolic coverage benchmark (fixed seeds, isolated caches, equal execution budgets, examples corpus + one downstream subset)"
priority: P1
type: task
labels: [audit-2026-09-22, concolic, benchmark, effectiveness]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [explore-stop-reason-accounting, explore-budget-semantics, concolic-fuzz-rng-unseeded]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Add a controlled default-vs-concolic coverage benchmark (fixed seeds, isolated caches, equal execution budgets, examples corpus + one downstream subset)

## Problem

README.md:1-3, SPEC.md, CLAUDE.md:3 and the CLI `--help` about text (`shatter-cli/src/args.rs:117`) describe Shatter as "automatic exploratory testing via concolic execution". The default explorer, however, is the adaptive random/hybrid scheduler (`explorer.rs`). The Z3-driven concolic orchestrator (`orchestrator.rs`) runs only with `--concolic` (SPEC.md:148, 713-715). Nobody has measured under controlled conditions whether `--concolic` finds more than the default:

- The audit's one fresh concolic run (21 hard TS functions) scored 164/454 lines (36.1%). The audit compared it with a default figure of 185/454 (40.7%), but no default-subset artifact for those 21 functions exists, so **the default baseline is unverified**. The run's per-function table is reproduced in concolic-early-termination. Its raw files are untracked (local to the audit worktree), and its command and examples SHA were not recorded.
- The maintainer's local agent memory for kapow (`~/.claude/projects/-home-ketan-project-shatter/memory/project_kapow_shatter_advise_log.md`, entry dated 2026-07-02; not in any repo) records "--concolic ... ZERO coverage delta vs random". That run was not controlled either.
- str-ior1 ("re-baseline Zolem with --concolic", P1) was closed with the reason "Closed" and no data.

Maintainer decision D3 (2026-09-23): **measure first**. This issue builds the benchmark harness and produces a first, pre-fix measurement. The post-fix measurement that the positioning decision uses is concolic-benchmark-postfix-run. The decision itself is concolic-positioning-decision. Do not change positioning docs here.

## Why this is blocked

Today no single entry point can run both arms under equal, reproducible conditions:

- **Stop reason and solver counts are constant in explore artifacts.** `ExploreResultAccumulator` drops them (explore-stop-reason-accounting).
- **Budgets are not equal.** `scan --concolic` gives the orchestrator `max_executions = 5 × --max-iterations` (`concolic_scan_max_executions`, `shatter-core/src/scan_orchestrator.rs:3144-3150`). `explore --concolic` uses 1× (`shatter-cli/src/commands/explore.rs:5145`), and `observe` uses 5×. No flag sets the execution budget directly (explore-budget-semantics).
- **Seeds do not control the concolic arm.** `--seed` exists only on `scan` (`args.rs:970-979`). Even there, the orchestrator's plateau fuzz phase draws from `StdRng::from_os_rng()` (`shatter-core/src/orchestrator.rs:3060`), ignoring the configured seed (concolic-fuzz-rng-unseeded).
- **Caches and resume leak across arms.** `scan` has no `--clean`. Its cache controls are `--no-cache` (behavior-map, fingerprint analysis and stored-inputs caches; `args.rs:1016`) and `--cache-dir`. Explore silently resumes prior results in the same artifact directory regardless of explorer mode (explore-resume-options-key).

## Evidence

- Reusable pieces:
  - `benchmarks/frontier-ranking/manifest.json` already defines seeds, regimes and fixtures with strata.
  - `task bench-frontier` / `bench-frontier-report` (Taskfile.yml:845-876) and `scripts/bench_frontier_report.py` run and report such benchmarks.
  - The existing frontier bench calls `orchestrator::explore_with_oracle` directly (`shatter-core/tests/bench_frontier_ranking.rs:381`), so it bypasses the CLI wiring that differs between modes. This benchmark must not.
- Examples corpus: the external checkout resolved by `scripts/examples_checkout.py` (unpinned `origin/main`; see pin-examples-repo).
- Line totals are suspect for the random explorer, which under-counts float-probe paths (float-probe-paths-uncounted). Branch outcomes are the primary metric.

## Acceptance criteria

- [ ] A task target (for example `task bench-explorers`) runs the **same** function manifest under the default explorer and under `--concolic`. It must:
  - go through one user-facing CLI entry point for both arms (`scan` or `explore`), never `orchestrator::` or `explorer::` directly;
  - pass an explicit execution budget that is **equal** for both arms, using the flag that explore-budget-semantics provides. The harness reads each arm's effective budget back from the run output and fails if the two differ;
  - use at least 3 fixed seeds per arm, passed with `--seed`;
  - give each arm × seed a fresh artifact directory and an empty `--cache-dir`, and pass `--no-cache` when using `scan`. The harness fails if any stderr contains `[resumed]` or `Found prior explore summary`.
- [ ] Reproducibility check in the harness: one arm × seed is run twice. The two per-function branch-outcome sets must be identical. If they are not, the harness fails and prints the first differing function.
- [ ] Corpus manifest (checked in):
  - every TS/Go/Rust examples-corpus function with `EXPECTED BRANCHES` comments, at a pinned examples SHA;
  - the audit's 21-function TS subset (listed in concolic-early-termination), as a named stratum;
  - one named subset of one downstream project (kapow, zolem or pickpackit), pinned by commit. Choose the project with the most functions that `shatter scan` completes without unsupported-target errors, and record that count in the manifest.
- [ ] Output, per function and in aggregate:
  - branch outcomes covered, and known-answer expected outcomes hit (primary);
  - lines covered (labelled secondary);
  - executions used, `stop_reason`, and `solver_guided_inputs`;
  - wall time;
  - mean and min/max across seeds.
- [ ] Every result file records its provenance: the shatter commit, the examples SHA, the downstream commit, the full command line for each arm, the effective budget, the seeds, and the host.
- [ ] A first pre-fix result is committed under `benchmarks/baselines/explorers/` and includes the default-explorer baseline for the 21-function subset. The close note states whether the audit's 40.7% default figure is confirmed or replaced.
- [ ] The release checklist (RELEASE docs or the release workflow) gains a step to run the benchmark and attach the results to the release notes.
- [ ] Proof at close: the command line and full output of one forced, uncached run from a clean checkout, the reproducibility-check output, and the committed results file.

## Suggested approach

1. Reuse the frontier-ranking manifest format and the `bench_frontier_report.py` style.
2. Keep per-function rows so that regressions such as classifyHttpResponse 13/13 -> 10/13 stay visible.
3. The benchmark may share a harness with effectiveness-benchmark-holdout, but it measures coverage per explorer, not bug-finding.

## Out of scope

- Fixing concolic early termination (concolic-early-termination, concolic-early-termination-fix).
- The post-fix measurement (concolic-benchmark-postfix-run).
- Changing README/SPEC/CLAUDE.md positioning (concolic-positioning-decision, D3).
- Fixing the blockers themselves: budget semantics, fuzz RNG seeding, accumulator fields, resume keying.

## Metadata

- Priority: P1 (raised from P2 by D3)
- Type: task
- Labels: audit-2026-09-22, concolic, benchmark, effectiveness
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: explore-stop-reason-accounting, explore-budget-semantics, concolic-fuzz-rng-unseeded
- Blocks: concolic-benchmark-postfix-run
- Related: concolic-early-termination, concolic-positioning-decision, effectiveness-benchmark-holdout, explore-resume-options-key, seed-for-explore-and-run (needed if the harness uses `explore`), pin-examples-repo, float-probe-paths-uncounted; str-ior1 (closed without data), str-2fui, str-qwua7.6
- Source findings: goals-08 (draft shatter-code/80)
- Decision refs: D3

---

---
slug: concolic-early-termination
kind: new
title: "Diagnose why --concolic explore runs end after 21-35 executions on most hard TS functions (diagnosis only; fix is concolic-early-termination-fix)"
priority: P1
type: bug
labels: [audit-2026-09-22, concolic, orchestrator, solver, diagnosis]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [explore-stop-reason-accounting]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Diagnose why --concolic explore runs end after 21-35 executions on most hard TS functions (diagnosis only; fix is concolic-early-termination-fix)

## Problem

On the audit's fresh `shatter explore --concolic` run over 21 hard TypeScript example functions, 12 functions stopped after 21-35 executions against a 100-iteration budget, and most of them left most branches uncovered. What ended those runs is not known yet, and the artifacts cannot answer it:

- **The artifacts' `stop_reason` and `solver_guided_inputs` are not trustworthy.** `ExploreResultAccumulator::into_result` (`shatter-cli/src/commands/explore.rs:190-330`) never copies `stop_reason` or `solver_guided_inputs` from the batch `ObservationOutput`. It fills both from `..Default::default()`, so every explore artifact reports `stop_reason: worklist_exhausted` (the `#[default]` variant, `shatter-core/src/explorer.rs:446-447`) and `solver_guided_inputs: 0`, whatever the engine did. That reporting bug is filed separately as explore-stop-reason-accounting and blocks this issue.
- **The solver is not idle.** The same artifacts record `z3` discoveries for 6 of the 21 functions (table below). An earlier draft of this issue claimed "zero solver-guided inputs", read from the defaulted field. That claim was wrong.

What remains unexplained is the early stop itself. Both "21 = 1 seed + `plateau_threshold` 20" (a coverage plateau, `orchestrator.rs:203`, `:1643-1647`) and "21 = seed-set size, then the worklist drains" fit the data. So does a correct stop, where the solver has nothing left to negate because the instrumentor emits no constraints for the branch (see Likely contributors). This issue decides which one applies. It is a diagnosis. The fix belongs to concolic-early-termination-fix, which this issue blocks.

## Evidence

Run (audit 2026-09-22, audit HEAD 56c86168): `shatter explore --concolic -w 4` over 9 files copied from the examples corpus `ts/` directory into a fresh directory. The files were 05-unions, 06-nested-control-flow, 07-auth-validation, 10-path-router, 14-semver, 15-email-validator, 17-mock-branches, 18-accept-language and 20-dotenv-parser. The exact command line and the examples SHA were not recorded. The raw files are local to the audit worktree and untracked: `audits/2026-09-22/goals-runs/` is listed in `.git/info/exclude`. Everything needed is therefore reproduced inline here.

Per-function results. `iters` and `paths` and `branches` come from the stderr `[batch]` lines. `raw` is `len(observation.raw_results)` from the artifact. `disc` is the artifact's `discoveries` grouped by method.

| Function | File | iters | raw | paths | branches | disc |
|---|---|---|---|---|---|---|
| computeArea | 05-unions.ts | 21 | 21 | 1 | 0/6 | none |
| routeRequest | 05-unions.ts | 26 | 26 | 2 | 1/8 | user 1 |
| classifyHttpResponse | 06-nested-control-flow.ts | 35 | 35 | 6 | 10/13 | z3 9, user 1 |
| processStateMachine | 06-nested-control-flow.ts | 100 | 43 | 3 | 2/12 | user 2 |
| validateJwt | 07-auth-validation.ts | 100 | 22 | 3 | 2/8 | user 2 |
| authorizeRequest | 07-auth-validation.ts | 29 | 29 | 3 | 3/14 | z3 2, user 1 |
| matchRoute | 10-path-router.ts | 21 | 21 | 1 | 1/19 | user 1 |
| resolveMiddleware | 10-path-router.ts | 29 | 29 | 5 | 6/7 | user 4, z3 2 |
| parseSemver | 14-semver.ts | 100 | 50 | 3 | 3/6 | user 3 |
| compareSemver | 14-semver.ts | 30 | 30 | 7 | 6/8 | user 6 |
| satisfiesRange | 14-semver.ts | 35 | 35 | 8 | 7/16 | user 7 |
| validateEmail | 15-email-validator.ts | 100 | 27 | 8 | 13/19 | z3 4, user 2, fuzzed 7 |
| classifyStatus | 17-mock-branches.ts | 21 | 21 | 1 | 0/3 | none |
| loadOrDefault | 17-mock-branches.ts | 21 | 21 | 1 | 1/2 | user 1 |
| classifyConfigs | 17-mock-branches.ts | 22 | 22 | 2 | 1/4 | user 1 |
| parsePreference | 18-accept-language.ts | 100 | 45 | 3 | 4/7 | user 4 |
| sortPreferences | 18-accept-language.ts | 95 | 95 | 13 | 2/2 | user 2 |
| negotiateLanguage | 18-accept-language.ts | 23 | 23 | 2 | 1/12 | user 1 |
| findSeparator | 20-dotenv-parser.ts | 29 | 29 | 3 | 2/2 | user 1, z3 1 |
| stripInlineComment | 20-dotenv-parser.ts | 100 | 100 | 25 | 5/5 | user 5 |
| parseDotenv | 20-dotenv-parser.ts | 100 | 27 | 6 | 7/12 | user 4, fuzzed 3 |

Observations:

- For 5 of the 100-iteration functions, `raw` is far below `iters` (for example validateJwt 100 vs 22). `iterations` is `total_executions` (`shatter-core/src/pipeline.rs:944`), so some execution phase (the plateau fuzz phase is the likely one) spends budget without recording `raw_results`. That is a separate accounting question worth answering here.
- stderr shows `Coverage plateau — entering fuzz phase targeting N opaque branch(es)` eight times (`ts-sub-concolic.err` lines 9, 14, 19, 33, 48, 62, 64, 76).
- The orchestrator's own termination reasons are set correctly at the terminating branch (`orchestrator.rs:1621-1647` returns `MaxIterations`/`MaxExecutions`/`TimeoutExplore`/`CoveragePlateau`, and `:3179` assigns it). `WorklistExhausted` (`:2557`) is only the initial value. The artifact values are wrong because of the CLI accumulator, not the orchestrator.

Durable reproduction, once explore-stop-reason-accounting lands: copy those 9 files from the examples checkout (`scripts/examples_checkout.py`; record its SHA) into an empty directory and run the command below. Then tabulate `iterations`, `stop_reason`, `solver_guided_inputs`, `len(raw_results)` and discoveries per artifact.

```
shatter explore --concolic -w 4 --max-iterations 100 <the 9 files>
```

### Likely contributors (link, do not duplicate)

- z3-mixed-int-real-sort-split (bucket shatter-engine-correctness): one parameter becomes two unrelated Z3 variables, so SAT models can produce unusable inputs.
- ts-switch-ternary-instrumentation (bucket shatter-frontend-ts): switch, ternary and value-position `&&`/`||` emit no `branch_path` decisions, so there is nothing to negate.
- known-answer-ratchet-and-ts-discriminants: computeArea's discriminant literal is widened to `str`, and the generated `{"kind":"true",...}` matches no case.

## Acceptance criteria

- [ ] A re-run of the 9-file subset, made after explore-stop-reason-accounting lands, is attached to the close note. It records the examples SHA, the shatter commit, the exact command, and a per-function table of `iterations`, `stop_reason`, `solver_guided_inputs`, `len(raw_results)` and discoveries by method.
- [ ] For computeArea, matchRoute, negotiateLanguage and classifyStatus, the close note states:
  - the actual `TerminationReason`, taken from a debug log of the orchestrator loop and not only from the artifact;
  - for each uncovered branch, why no solver-guided input reached it. The reason is one of: no `SymExpr` path constraint emitted; Z3 unsat, unknown or error (quoted); model extraction dropped or mis-sorted values; or the input was generated and executed but took the same path.
- [ ] The close note explains the `iterations` > `len(raw_results)` gap: which phase consumes executions without recording them, and whether that is intended.
- [ ] The close note ends with a verdict of **defect** or **expected**, with the reasoning. For **defect**, it names the code sites and a minimal fixture (function body plus expected branch) for concolic-early-termination-fix to use as its known-answer test. For **expected**, it lists the linked issues that account for each loss, and concolic-early-termination-fix is closed with a link to this note.
- [ ] Every loss attributed to z3-mixed-int-real-sort-split, ts-switch-ternary-instrumentation or known-answer-ratchet-and-ts-discriminants is added as a comment on that issue, with the function name and evidence.

## Suggested approach

1. Reproduce one function (computeArea or matchRoute) with `RUST_LOG=debug` in a fresh directory. Log each worklist push and pop with its source (seed, z3, boundary, drill, fuzz), and each solver call result (sat, unsat, unknown, error).
2. Check whether the branch decisions carry `SymExpr` path conditions. If they do, check the Z3 results and model extraction.

## Out of scope

- Any code fix (concolic-early-termination-fix).
- The artifact field accounting (explore-stop-reason-accounting).
- The benchmark (concolic-vs-default-benchmark) and the positioning decision (concolic-positioning-decision).

## Metadata

- Priority: P1 (D3)
- Type: bug (diagnosis)
- Labels: audit-2026-09-22, concolic, orchestrator, solver, diagnosis
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: explore-stop-reason-accounting
- Blocks: concolic-early-termination-fix
- Related: z3-mixed-int-real-sort-split, ts-switch-ternary-instrumentation, known-answer-ratchet-and-ts-discriminants, concolic-vs-default-benchmark
- Source findings: goals-08 (split from draft shatter-code/80)
- Decision refs: D3

---

---
slug: concolic-positioning-decision
kind: new
title: "Decision: keep or revise the 'concolic-first' product positioning in README/SPEC, using the post-fix default-vs-concolic benchmark results"
priority: P2
type: decision
labels: [audit-2026-09-22, concolic, docs, decision]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [concolic-benchmark-postfix-run]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Decision: keep or revise the 'concolic-first' product positioning in README/SPEC, using the post-fix default-vs-concolic benchmark results

## Problem

Shatter presents itself as concolic-first, but the default explorer is not the concolic engine, and the one audit measurement of `--concolic` was uncontrolled (goals-08). Maintainer decision D3 (2026-09-23) is to measure first and then decide. This issue holds that decision.

It is blocked on concolic-benchmark-postfix-run. That issue runs the controlled benchmark (concolic-vs-default-benchmark) after the early-termination fix (concolic-early-termination-fix) has landed, and commits the result. The decision therefore rests on a working engine and controlled numbers, and this issue does not run anything itself.

## Evidence (current positioning, audit HEAD 56c86168)

- `README.md:3`: "Automatic exploratory testing via concolic execution."
- `CLAUDE.md:3`: the same sentence.
- `shatter-cli/src/args.rs:117`: CLI about text, "Shatter: automatic exploratory testing via concolic execution."
- `SPEC.md:148`: `--concolic` (default false) "Use the concolic (Z3-backed) explorer instead of the random explorer."
- `SPEC.md:703-716`: the default adaptive scheduler blends user, boundary, solver-guided and random inputs; `--concolic` is listed as an alternative explorer.
- str-jd0d1 ("Walkthrough labels random runs concolic", open P2) is a live instance of the same mismatch between the label and the engine that actually ran.
- Audit run: 36.1% lines on 21 functions under `--concolic`. The 40.7% default baseline was not reproduced, and the explore artifacts' stop reasons were constant (explore-stop-reason-accounting). The audit run is not evidence for either side.

## Acceptance criteria

- [ ] The decision is recorded in this issue and cites the committed post-fix results file from concolic-benchmark-postfix-run by path and commit. It is one of:
  - (a) keep "concolic-first";
  - (b) reposition (for example "solver-assisted exploratory testing", with `--concolic` as an option);
  - (c) make `--concolic` the default.
- [ ] The decision states the metric it relies on (primary: branch outcomes covered and known-answer expected outcomes hit, as mean across seeds), the threshold, and the observed values for both arms. For example: "(c) requires concolic ≥ default on the aggregate and on each language stratum, and no function regressing by more than N branch outcomes".
- [ ] If (b) or (c): follow-up issue(s) are filed for the README, SPEC, CLAUDE.md and `--help` text and for any default change, each naming the exact files and lines, and linked here. str-jd0d1 is linked as related.
- [ ] If (a): the issue states which benchmark result justifies the wording, and names the per-release benchmark check that would reopen the decision if concolic falls below the threshold.

## Suggested approach

Review the per-function rows as well as the aggregate. Mixed results (for example validateEmail 6/19 -> 13/19 against classifyHttpResponse 13/13 -> 10/13 in the audit run) may favor a hybrid positioning over a binary answer.

## Out of scope

- Editing README/SPEC now. D3 forbids doc softening before the measurement.
- Running the benchmark (concolic-benchmark-postfix-run).
- Fixing the engine.

## Metadata

- Priority: P2
- Type: decision
- Labels: audit-2026-09-22, concolic, docs, decision
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: concolic-benchmark-postfix-run
- Related: concolic-vs-default-benchmark, concolic-early-termination-fix, str-jd0d1 (open), str-ior1 (closed without data)
- Source findings: goals-08
- Decision refs: D3

---

---
slug: effectiveness-benchmark-holdout
kind: new
title: "Deliver a minimal bug-finding effectiveness benchmark with a defined scoring oracle (seeded faults, detection rule, denominator, false-positive control)"
priority: P2
type: task
labels: [audit-2026-09-22, effectiveness, benchmark]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Deliver a minimal bug-finding effectiveness benchmark with a defined scoring oracle (seeded faults, detection rule, denominator, false-positive control)

Scope note: the slug is kept from the earlier combined draft. The holdout disposition moved to holdout-disposition. Tracker initialization and backlog migration for shatter-effectiveness moved to effectiveness-repo-tracker-backlog. This issue is the benchmark itself.

## Problem

Shatter's effectiveness measurement ("does it find real bugs?") has been designed three times and never delivered, so nobody can tell whether engine changes improve results.

- **holdout** (`~/project/holdout`) last ran on 2026-04-09, and its totals are wrong (see holdout-disposition).
- **shatter-effectiveness** (`~/project/shatter-effectiveness`) contains a 1,057-line design (`docs/specs/2026-08-27-effectiveness-benchmark-design.md`) and a 1,260-line 13-task plan (`docs/superpowers/plans/2026-08-30-effectiveness-benchmark.md`). It holds only `docs/ scripts/ tests/` (a docs-integrity gate) and no benchmark code. Last commit: 6e234fe, 2026-08-31, "Merge implementation-plan".
- The downstream ≥90% coverage goals (kapow, zolem, pickpackit) have stalled at 18-28% since 2026-07-07, with no metric linking engine work to outcomes (downstream-coverage-goals-epic).

This issue lives in the shatter tracker because shatter-effectiveness has no tracker and holdout's bd has no issues.

It is distinct from concolic-vs-default-benchmark, which measures **coverage per explorer**. This issue measures **fault detection**. The two may share a harness (corpus checkout, manifest format, run and record scripts).

## Evidence (re-verified 2026-09-23)

```
$ git -C ~/project/shatter-effectiveness log -1 --format='%h %ad %s' --date=short
6e234fe 2026-08-31 Merge implementation-plan: thirteen-task plan for the benchmark
$ ls ~/project/shatter-effectiveness
docs  scripts  tests
```

## Scoring oracle (required, fixed before implementation)

The benchmark's result is a fault-detection rate, defined as follows. The implementer may change the details, but only by recording the change in this issue before the first result is committed.

- **Seeded faults.** Each fault is a single, reviewed source mutation of a known function (operator swap, off-by-one boundary, dropped branch, wrong constant). Each is stored as a patch in a fault manifest with an ID, target function, language and fault class. At least 20 faults, over at least 2 languages.
- **Observable failure.** A fault counts as **detected** when a Shatter run on the mutated source produces at least one input whose observed outcome (return value, thrown error, or declared side effect) differs from the outcome of the **unmutated** source on that same input. The harness replays each generated input against both versions to decide this. It may reuse `shatter spec-diff` (the regression tool per D2) only where its branch-path comparison gives the same answer as this input-level rule. Only the run's own generated inputs count. Inputs copied in from the fault manifest do not.
- **Denominator.** Faults whose target Shatter can execute on the unmutated source. A fault whose target is unsupported (analysis or execution error on the clean source) is reported as **unsupported** and excluded from the rate, never counted as missed.
- **False-positive control.** Every run also scores the unmutated source against itself with the same seed. Any reported difference is a false positive and is listed. The run fails if the false-positive count is non-zero, unless each one is traced to a documented nondeterminism source.
- **Equivalent mutants.** A fault that no input can distinguish is marked equivalent in the manifest after review, and excluded.

## Acceptance criteria

- [ ] Location recorded in this issue before code lands: a `task` target inside shatter, or shatter-effectiveness. Reasons given: tracker, CI access, corpus pinning.
- [ ] Fault manifest checked in with at least 20 faults over at least 2 languages, each with ID, target, class and patch. Targets come from the examples corpus (pinned SHA) and, optionally, one downstream project (pinned commit).
- [ ] An on-demand command runs every fault plus the unmutated control with at least 3 seeds and writes a dated result file. Per fault the file reports detected, missed, unsupported or equivalent, with the distinguishing input when detected. In aggregate it reports detection rate (with the denominator), unsupported count and false-positive count, as mean and min/max across seeds. The file records provenance: shatter commit, corpus SHA, commands and seeds.
- [ ] Self-test of the oracle: one fault known to be trivially detectable (for example a changed return constant on an unconditional path) is detected, and the unmutated control reports 0 differences. If either check fails, the command exits non-zero.
- [ ] If the harness is shared with concolic-vs-default-benchmark, the shared parts are named in both issues.
- [ ] Proof at close: the command and full output of one uncached run, and the committed or published dated result file.

## Out of scope

- holdout (holdout-disposition).
- Tracker setup and the rest of the 13-task plan (effectiveness-repo-tracker-backlog).
- Coverage comparison between explorers (concolic-vs-default-benchmark).
- Downstream coverage-goal work.

## Metadata

- Priority: P2
- Type: task
- Labels: audit-2026-09-22, effectiveness, benchmark
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Blocks: holdout-disposition, effectiveness-repo-tracker-backlog
- Related: concolic-vs-default-benchmark (possible shared harness, different metric), downstream-coverage-goals-epic, pin-examples-repo, retire-snapshot-diff (D2: spec-diff is the regression tool); kapow-94wr (earlier "eval harness unreliable" symptom); str-jeen.14 (closed, broad-run validation corpus)
- Source findings: goals-10 (draft other-first-party/50)

---

---
slug: engine-path-identity-budget-config
kind: new
title: "Random explorer and concolic orchestrator use different path identities (bucketed path_hash vs raw hash_branch_path); define one canonical identity and test it on shared traces"
priority: P2
type: task
labels: [audit-2026-09-22, architecture, parity, explorer, orchestrator]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Random explorer and concolic orchestrator use different path identities (bucketed path_hash vs raw hash_branch_path); define one canonical identity and test it on shared traces

Scope note: the slug is kept from the earlier combined draft. This issue now covers **path identity only**. Budget semantics moved to explore-budget-semantics. The scan/observe `orchestrator::ExploreConfig` literals moved to a note on str-qwua7.6.2 (qwua7-6-2-scan-observe-config-literals). Relocating `hash_branch_path` into a leaf module is already str-qwua7.29 and is not repeated here.

## Problem

The two engines decide "is this a new path?" with different functions:

- The random explorer uses `path_hash` (`shatter-core/src/explorer.rs:566`): scope-aware, loop iterations collapsed into buckets (`LoopBuckets`), with a `legacy_path_hash` fallback (`:575`) over lines, error and return value when there is no `branch_path`.
- The concolic orchestrator uses `hash_branch_path` (`shatter-core/src/orchestrator.rs:864`): the raw `(branch_id, taken)` sequence through `DefaultHasher`, with no loop bucketing and no fallback. It drives the unique-path budget (`orchestrator.rs:1621-1627`, via `:1736`) and fuzz-phase novelty (`:3002`, `:3113`).
- The random explorer's own shrink-witness selection calls the concolic hash, `crate::orchestrator::hash_branch_path`, at `explorer.rs:1704`, `:1788`, `:1833` and `:1886`. So one engine uses two identities.

Consequences: "paths" in a report mean different things depending on `--concolic`. A loop function inflates the concolic path count, which also consumes the concolic unique-path budget. The random explorer's shrinker can pick a witness for a path that its own explorer treats as a duplicate, or the reverse.

## Evidence

The audit's fixture (`audits/2026-09-22/areas/core-engine.md:10-25`; not in any test directory):

```go
func Loopy(n int) int {
    s := 0
    for i := 0; i < n && i < 50; i++ { s += i }
    if s > 100 { return 1 }
    return 0
}
```

Result: 16 paths under `--concolic` (40 iterations) against 6 under random (100 iterations), for 2 return behaviours. The different budgets and inputs mean this comparison **illustrates** the problem but does not prove it. The proof in the acceptance criteria below uses identical traces.

## Acceptance criteria

- [ ] A written definition of the canonical path identity, in the doc comment of the one function that computes it. It states whether loops are bucketed (and with which `LoopBuckets`), how scopes are treated, and what the fallback is when `branch_path` is empty. The default proposal is the random explorer's `path_hash` semantics. Choosing otherwise needs a reason in the close note.
- [ ] Both engines (the random explorer's novelty check, the concolic orchestrator's unique-path budget and fuzz novelty, and both shrink-witness selections) call that one function. `grep -n "hash_branch_path" shatter-core/src` finds no call site used for path novelty or witness selection. Remaining uses, if any, are listed in the close note with the reason.
- [ ] Trace-level tests feed the **same** `ExecuteResult` values to the identity function as each engine calls it, and assert equal identities. Cases:
  - two Loopy traces with 3 and 4 loop iterations that fall in the same bucket (same identity);
  - two traces whose loop counts fall in different buckets (different identities);
  - a trace with an empty `branch_path` (fallback path);
  - a trace with nested scopes.

  These tests do not compare discovered path counts between engines, because the two engines legitimately explore different inputs under finite budgets.
- [ ] Loopy is added as a checked-in fixture (for example `examples/go/` via the examples repo, or a core test fixture directory), with its expected distinct identities (2 return behaviours; the bucketed loop-count identities written out).
- [ ] Engine-level coverage expectations are separate from identity: an engine-parity row (engine-parity-e2e) asserts that both engines reach both return behaviours of Loopy under a stated budget, not that they report equal path counts.
- [ ] Proof at close: the trace-level test fails on current main for the concolic call site (paste the assertion output) and passes after the change, and forced (uncached) `task e2e` output.

## Suggested approach

If str-qwua7.29 has landed, put the canonical function in its leaf module. If not, put it in `explorer.rs` next to `path_hash` and let str-qwua7.29 move it. Either way, this issue changes semantics and call sites, and str-qwua7.29 owns module location.

## Out of scope

- Moving `hash_branch_path` or other leaf types out of `orchestrator.rs` (str-qwua7.29).
- `--max-iterations` and `max_executions` semantics (explore-budget-semantics).
- Unifying the `orchestrator::ExploreConfig` construction (str-qwua7.6.2; see qwua7-6-2-scan-observe-config-literals).
- Splitting `explore_with_oracle` (str-qwua7.6).

## Metadata

- Priority: P2
- Type: task
- Labels: audit-2026-09-22, architecture, parity, explorer, orchestrator
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: str-qwua7.29 (open, owns relocation), str-qwua7.6.2, str-inct, explore-budget-semantics, engine-parity-e2e, float-probe-paths-uncounted
- Source findings: core-07 (draft shatter-code/17)

---

---
slug: engine-parity-e2e
kind: new
title: "Add an engine_parity E2E suite that runs fixtures under random and concolic through the production pipeline, with strict expected-divergence markers"
priority: P2
type: task
labels: [audit-2026-09-22, parity, e2e, testing]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Add an engine_parity E2E suite that runs fixtures under random and concolic through the production pipeline, with strict expected-divergence markers

Scope note: the slug is kept from the earlier combined draft. The `_`-binding lint moved to underscore-binding-lint. The CLAUDE.md close-reason rule moved to pipeline-close-reason-rule. The per-BranchType TS fixtures are ts-branchtype-known-answer-fixtures (bucket shatter-frontend-ts).

## Problem

CLAUDE.md warns repeatedly about parallel code paths (the random explorer vs the concolic orchestrator, and the CLI wiring for `--concolic` vs the default), but the only enforcement is "grep for the parallel path". This audit found at least seven random-vs-concolic drifts that survived, each filed separately:

- `--setup` is ignored under concolic (concolic-setup-teardown);
- dynamic mock variation regressed (concolic-mock-variation-regression);
- the refine phase drops `prepare_id` and `execution_profile` (concolic-refine-execute-builder);
- shrinking runs after teardown;
- capture is hard-coded (str-qwua7.5);
- path identity differs (engine-path-identity-budget-config);
- float-probe paths are under-counted (float-probe-paths-uncounted).

Two issues were also closed on evidence from non-production paths: str-0s76.6, via a test that calls `orchestrator::explore` directly, and str-55ep, which fixed a dead shrinker copy.

## Evidence (re-verified at audit HEAD 56c86168)

- `shatter-core/tests/e2e_concolic.rs:1553-1558`: `orchestrator_explore_with_setup_context` ("This is the parity test for the orchestrator path") injects setup context straight into `orchestrator::explore`. The production callers (`pipeline_orchestrator.rs:542`, `scan_orchestrator.rs:3109`, `observe.rs:186`, per core-03) pass `None`.
- `grep -c "pipeline_orchestrator\|run_pipeline"` gives `e2e_concolic.rs` 1, `e2e_concolic_go.rs` 0 and `e2e_concolic_rust.rs` 0. The Go and Rust E2E suites never exercise the pipeline wiring where drift happens.

## Acceptance criteria

- [ ] New E2E suite `shatter-core/tests/engine_parity.rs`, wired into `task e2e`. It runs a table of fixtures × {random, concolic} through `pipeline_orchestrator`/`run_pipeline` or the CLI entry point. It never calls `orchestrator::explore` or `explorer::explore_function` directly; a grep in the close note shows this. It covers at least one TS, one Go and one Rust fixture.
- [ ] Each row asserts, for both engines, under a stated budget and fixed seed:
  - every expected return behaviour or branch outcome of the fixture is reached (coverage expectation, not equal path counts);
  - a `--setup` side effect is visible to the function under test;
  - a mock-dependent branch is reached;
  - the capture setting is honoured.
- [ ] **Expected-divergence semantics.** A known drift is recorded as a marker on one row and one named assertion: `expect_divergence(assertion = "setup_visible", engine = Concolic, issue = "str-...")`. The rules:
  - every assertion in the row still executes; there is no `#[ignore]` and no blanket allowance;
  - the marked assertion must **fail** in the marked engine. If it passes, the test fails with "unexpected pass: remove the marker and close <issue>";
  - every unmarked assertion in the row must pass, so an unrelated failure is never hidden by the marker;
  - the suite prints, at the end, a count of passing, expected-divergent and unexpected-pass assertions.
- [ ] A self-test of the harness shows all three cases: a row with a marker on an assertion that passes fails the suite; a marked row with an unrelated failing assertion fails the suite; a correctly marked row passes.
- [ ] Every drift in the Problem list that is reachable from these fixtures has a row, either passing or marked with its issue ID.
- [ ] Proof at close: forced (uncached) `task e2e` output showing `engine_parity` executed, with its pass / expected-divergent / unexpected-pass counts, and the harness self-test output.

## Suggested approach

Build the scaffold and the TS rows first, then add Go and Rust rows. The Loopy row from engine-path-identity-budget-config checks that both return behaviours are reached, not path counts.

## Out of scope

- Fixing the individual engine drifts (filed separately).
- The `_`-binding lint (underscore-binding-lint) and the close-reason rule (pipeline-close-reason-rule).
- Per-BranchType TS fixtures (ts-branchtype-known-answer-fixtures).
- The module-reachability check (core-reachability-gate) and the module-graph cycle check (str-qwua7.29).

## Metadata

- Priority: P2
- Type: task
- Labels: audit-2026-09-22, parity, e2e, testing
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: ts-branchtype-known-answer-fixtures, engine-path-identity-budget-config, concolic-setup-teardown, concolic-mock-variation-regression, concolic-refine-execute-builder, float-probe-paths-uncounted, underscore-binding-lint, pipeline-close-reason-rule; str-qwua7.29, str-qwua7.51, str-inct, str-qwua7.5
- Source findings: core-22 (draft shatter-agent/23, split)

---

---
slug: core-dead-code-removal
kind: new
title: "Remove ~3,400 lines of production-dead shatter-core code (recursive, array_mutation, reporter+clustering, sequential scan(), shrink_witness)"
priority: P2
type: chore
labels: [audit-2026-09-22, cleanup, shatter-core, tech-debt]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Remove ~3,400 lines of production-dead shatter-core code (recursive, array_mutation, reporter+clustering, sequential scan(), shrink_witness)

Scope note: the slug is kept. The reachability gate that was part of this draft is now core-reachability-gate.

## Problem

Several shatter-core modules and functions have no non-test callers, yet they attract fixes, audit grades and planned proptests as if they were live, and they make parity reasoning harder. Examples:

- str-8q1b4 cites the dead sequential `scan()` as a fix site.
- str-qwua7.47 plans proptests for `array_mutation`.
- The prior audit graded clustering "Solid".

Only `export.rs` (1,743 lines) is tracked for deletion (str-qwua7.59).

## Evidence (re-verified at audit HEAD 56c86168)

| Item | Size | Callers outside own file / tests |
|---|---|---|
| `shatter-core/src/recursive.rs` | 675 lines | none |
| `shatter-core/src/array_mutation.rs` | 397 lines | none |
| `shatter-core/src/reporter.rs` | 1,326 lines | none |
| `shatter-core/src/clustering.rs` | 530 lines | only `reporter.rs` |
| `scan_orchestrator::scan()` (`scan_orchestrator.rs:1480`) | 482 lines | only the test at `scan_orchestrator.rs:7510` (helper doc at `:1223` says "Used by the non-parallel `scan()` path") |
| `shrink::shrink_witness` (`shrink.rs:40`) | — | tests only |
| `input_gen::mutate_mock_values` (`input_gen.rs:4241`) | — | tests only (`:8057`, `:8523`) |

The total is about 3,410 lines, excluding `export.rs`.

Check used, over the whole workspace (every crate's `src/`, `tests/` and `benches/`, and `examples/`, excluding `target/`): `grep -rln "recursive::\|array_mutation::\|reporter::\|clustering::\|shrink_witness\|mutate_mock_values" --include=*.rs`. It matches only `shatter-core/src/reporter.rs` (which uses clustering), `shrink.rs` and `input_gen.rs` (the items' own files). Test-only use inside those files is in `#[cfg(test)]` blocks.

## Acceptance criteria

- [ ] Each item above is either deleted, or wired into production behind a tracked issue whose ID appears in a comment at the item.
- [ ] `mutate_mock_values`: coordinate with concolic-mock-variation-regression. If that fix revives it as the production mock-variation path, it stays. Otherwise it is deleted.
- [ ] The workspace-wide grep above, re-run on the final branch, finds no reference to a deleted item. The output is in the close note.
- [ ] Note appended to str-qwua7.47 (open): drop `array_mutation` from its proptest list, because the module is deleted here.
- [ ] Proof at close: `cargo build --workspace --all-targets` and forced (uncached) `task check` output after the deletions.

## Suggested approach

One commit per module. Delete `reporter.rs` and `clustering.rs` together. Delete `scan()` together with its test and the doc reference at `:1223`.

## Out of scope

- The reachability gate (core-reachability-gate).
- Deleting `export.rs` (str-qwua7.59).
- Refactors of live code in the same files.

## Metadata

- Priority: P2
- Type: chore
- Labels: audit-2026-09-22, cleanup, shatter-core, tech-debt
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: core-reachability-gate, str-qwua7.59, str-qwua7.47, str-8q1b4, concolic-mock-variation-regression, engine-parity-e2e
- Source findings: core-08, core-17 (array_mutation part) (draft shatter-code/18)

---

---
slug: stable-hash-persisted-keys
kind: new
title: "std DefaultHasher keys persisted or promised-reproducible values (scan --seed sampling, Rust harness caches, external-audit cache dir, run scope_hash, ExecutionRecord.input_hash); use a specified hash"
priority: P3
type: bug
labels: [audit-2026-09-22, rust, cache, reproducibility]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# std DefaultHasher keys persisted or promised-reproducible values (scan --seed sampling, Rust harness caches, external-audit cache dir, run scope_hash, ExecutionRecord.input_hash); use a specified hash

## Problem

The std docs say the `DefaultHasher` algorithm is unspecified and may change between Rust releases. Shatter uses it for values that are written to disk, used in cache paths, or promised reproducible. The repo has no `rust-toolchain.toml`, so a toolchain bump can silently change which functions `scan --seed` / `--batch next` select, and re-key or orphan on-disk caches.

Impact is bounded: `DefaultHasher::new()` uses fixed SipHash keys, so results are stable within one toolchain. The practical effects are spurious rebuilds, orphaned cache directories, run manifests whose `scope_hash` changes with no scope change, and non-reproducible sampling across toolchains. Wrong results would require a collision.

## Migration surface (full inventory, audit HEAD 56c86168)

Found with `grep -rn "DefaultHasher" --include=*.rs` over every workspace crate's `src/` (non-test code). This table is the whole surface. The implementer confirms each classification and records any change in the close note.

**Must migrate (persisted, cache path, or promised reproducible):**

| Site | Use | Why it counts |
|---|---|---|
| `shatter-core/src/core_sample.rs:389` `default_seed`, `:550` `stable_hash` | core-sample selection | `--seed` help promises reproducibility (`shatter-cli/src/args.rs:970-979`) |
| `shatter-rust/src/executor.rs` `native_replay_hash` (`:306-308`), `mocks_hash` (`:736-739`), `source_hash` (`:764-766`, folds in `HARNESS_CACHE_VERSION = 2` from `:758`), `stable_crate_harness_dir` (`:839-847`), `stable_crate_bridge_dir` (`:3234-3236`), `hash_native_replay_sources` (`:3734-3749`), `runtime_source_hash` (`:4139-4175`) | harness cache keys and directories | compiled binaries reused from disk |
| `shatter-cli/src/helpers.rs:711-721` `external_audit_cache_root` | `$TMP/shatter-audit-cache/<hash>/harness` | cache path |
| `shatter-cli/src/commands/run.rs:1041` `scope_hash` | `scope_hash` field of the run manifest (`run.rs:237`, `:274`) | written to disk; its doc comment only promises reproducibility "within a single Shatter version", which a toolchain bump breaks |

**Must classify (persisted status to be confirmed):**

| Site | Use |
|---|---|
| `shatter-core/src/pipeline.rs:794`, `invariants.rs:754`, `scan_orchestrator.rs:1287` | `ExecutionRecord.input_hash`. `ExecutionRecord` is `Serialize` (`execution_record.rs:161-165`), and `behavior.rs:264-271` dedups behaviour maps by it. If behaviour maps or records reach the on-disk behaviour-map cache, this migrates. |
| `shatter-core/src/scan_orchestrator.rs:1343` `detect_mock_misses` | `caller_execution_id` on `MockMiss`. Migrates if written to reports. |

**May keep `DefaultHasher` (in-memory only):** `orchestrator.rs:865` `hash_branch_path`; `explorer.rs:359` `path_feedback_fingerprint`, `:576` `legacy_path_hash`, `:757`/`:799` (loop collapse), `:2212` `candidate_fingerprint`; `float_probe.rs:157`; `recursive.rs:541` (module deleted by core-dead-code-removal). `shrunk_witnesses`, which is keyed by path hash, is not serialized into explore artifacts (checked on the audit's artifacts).

`shatter-rust/src/executor.rs:6208-6218` `compute_prepare_id` already uses `sha2::Sha256`.

## Acceptance criteria

- [ ] Every site in "Must migrate", and every "Must classify" site confirmed as persisted, uses a specified algorithm over an explicit byte encoding. Options: SHA-256 truncated to u64 (sha2 is already a dependency), FNV-1a, or fixed-key `siphasher`.
- [ ] The close note contains the final classification table (every `DefaultHasher` site in non-test code, marked migrated or kept with a reason). `grep -rn "DefaultHasher" --include=*.rs` over the workspace on the final branch matches that table exactly.
- [ ] `HARNESS_CACHE_VERSION` is bumped once. The run manifest records a scope-hash algorithm version (or a new field name) so old and new manifests are not compared as if equal.
- [ ] Golden-value unit tests pin the output for fixed inputs of each migrated function (at least `default_seed("x")`, `stable_hash(...)`, `source_hash("...")`, `scope_hash(<fixed scope>)`, `external_audit_cache_root` for a fixed path), so an algorithm change fails a test. Proof at close: test output.
- [ ] The rust-conventions skill (`.claude/skills/rust-conventions/`) gains a one-line rule: no `DefaultHasher` for persisted, cache-path or promised-stable keys.

## Suggested approach

A small `stable_hash_u64(&[u8]) -> u64` helper per crate, or one shared helper if a common crate fits the dependency direction (cli -> core; frontends -> protocol).

## Out of scope

- Adding a rust-toolchain pin (separate decision). Without a pin, the golden tests catch algorithm changes only on the toolchain CI uses; that is acceptable.
- Hashes that are never persisted.

## Metadata

- Priority: P3
- Type: bug
- Labels: audit-2026-09-22, rust, cache, reproducibility
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: str-0m0vn (closed; --seed reproducibility), seed-for-explore-and-run, core-dead-code-removal
- Source findings: core-20, frontend-rust-13 (draft shatter-code/25)

---

---
slug: qwua7-6-function-length-ratchet
kind: note-to-existing
title: "Note on str-qwua7.6: explore_with_oracle grew 1,307 -> 1,372 lines; add a function-length ratchet"
priority: P3
type: note
labels: [audit-2026-09-22, tech-debt, shatter-core]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.6
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.6: explore_with_oracle grew 1,307 -> 1,372 lines; add a function-length ratchet

**Target:** `str-qwua7.6` (open). Action: append the comment below. Do not create a new issue. Suggested priority for the target is unchanged. This note adds P3 scope.

## Comment text

Audit 2026-09-22 note (finding core-15; evidence `audits/2026-09-22/areas/core-engine.md`).

**Current sizes** (audit HEAD 56c86168; brace-matched from the `fn` line to the closing `}`):

- `orchestrator::explore_with_oracle` (`shatter-core/src/orchestrator.rs:2490`): **1,372 lines**, up from 1,307 at the 2026-09-04 audit.
- `scan_orchestrator::parallel_scan_with_progress` (`scan_orchestrator.rs:3806`): 1,346.
- `explorer::explore_function` (`explorer.rs:1012`): 965.
- `scan_orchestrator::run_layer_batched` (`scan_orchestrator.rs:2461`): 476.

The shrink-witness selection block is still duplicated between `explorer.rs:~1700-1740` and `orchestrator.rs:~3561-3600`; both call `hash_branch_path`.

Features keep landing in the god function while this epic's P1 children stay open.

**Proposed additions to this issue's acceptance:**

- [ ] Add a function-length ratchet to `task check-static`. A script records the current length of each listed function (at least the four above) in a checked-in baseline and fails when any of them grows. Shrinking updates the baseline. Proof: the script fails on a branch that adds a line to `explore_with_oracle`.
- [ ] After str-qwua7.6.1 (`select_witnesses`) lands, plan the phase extraction of `explore_with_oracle` (probe, main loop, refine, shrink) as child issues. str-qwua7.6.3 covers the CLI's `run_explore`, not this function.

---

---
slug: qwua7-43-bench-dev-dep-cycle
kind: note-to-existing
title: "Note on str-qwua7.43: bench_frontier_ranking.rs deepened the core -> shatter-llm dev-dependency cycle"
priority: P2
type: note
labels: [audit-2026-09-22, architecture, agents]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.43
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.43: bench_frontier_ranking.rs deepened the core -> shatter-llm dev-dependency cycle

**Target:** `str-qwua7.43` (open, decided 2026-09-05). Action: append the comment below. Do not create a new issue.

## Comment text

Audit 2026-09-22 note (finding frontend-rust-09; evidence `audits/2026-09-22/areas/frontend-rust.md`).

**What changed since this issue was decided:**

- The Jev frontier-ranking benchmark work (str-hjrnp.3, closed) added `shatter-core/tests/bench_frontier_ranking.rs`. It imports `shatter_llm::{DecisionFrontierRanker, JevAdapter, MockSeedOracle, ReplayDecisionOracle}` and `shatter_llm::jev::JevConfig` (lines 34-35), and calls `shatter_llm::build_oracle` (line 304).
- Its plan (`docs/superpowers/plans/2026-09-21-jev-frontier-ranking-benchmark.md`, around line 1037) explicitly said to add shatter-llm to core's `[dev-dependencies]`. That contradicts this issue's decision.
- Core now has two llm-dependent test files: `e2e_llm_oracle.rs` and `bench_frontier_ranking.rs`. The dev-dependency is still at `shatter-core/Cargo.toml:44` (`shatter-llm = { path = "../shatter-llm" }`).
- str-6nul9 (landed in 20692b08, merged 70465921) excluded `bench_frontier_ranking` from `core:test-ignored`'s sweep. The gate timeout is gone, but the dependency edge remains.

**Scope addition:**

- [ ] Move `bench_frontier_ranking.rs`, together with `e2e_llm_oracle.rs`, into `shatter-llm/tests/` or a dedicated bench crate. Then drop the dev-dependency at `shatter-core/Cargo.toml:44`.
- [ ] Update the Taskfile targets that name the test by crate: `bench-frontier-reference` (`Taskfile.yml:852`), `bench-frontier` (`:865`), and any `-p shatter-core --test bench_frontier_ranking` references. Also update the str-6nul9 exclusion, so the benchmark stays out of gate sweeps in its new home.
- [ ] Proof at close:
  - `cargo tree -p shatter-core -e dev | grep shatter-llm` prints nothing;
  - `task bench-frontier-reference` still runs from the new location (command + output).

The process gap (planning did not check open decided issues touching the same dependency edge) is filed separately as planning-rules-location-and-open-decisions (bucket shatter-agent-guidance-and-repo-hygiene) and is not part of this issue.

---

---
slug: explore-stop-reason-accounting
kind: new
title: "explore artifacts always report stop_reason worklist_exhausted and solver_guided_inputs 0: ExploreResultAccumulator drops both fields"
priority: P1
type: bug
labels: [audit-2026-09-22, concolic, cli, artifacts, explore]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore artifacts always report stop_reason worklist_exhausted and solver_guided_inputs 0: ExploreResultAccumulator drops both fields

## Problem

`shatter explore` merges per-batch `ObservationOutput`s through `ExploreResultAccumulator` (`shatter-cli/src/commands/explore.rs:190-330`). The accumulator has no field for `stop_reason` or `solver_guided_inputs`. `into_result` builds the final `ObservationOutput` with `..Default::default()`. So every explore artifact and report shows:

- `stop_reason: worklist_exhausted`, the `#[default]` variant of `StopReason` (`shatter-core/src/explorer.rs:435-447`);
- `solver_guided_inputs: 0`.

This happens on both engines and whatever actually ended the run. The engine computes both values correctly: `pipeline.rs:951` sets `solver_guided_inputs = z3_generated + boundary_generated + drill_generated`, and the orchestrator returns the real `TerminationReason` (`orchestrator.rs:1621-1647`, `:3179`). The CLI then discards them.

Impact: the audit misread these fields as "the Z3 loop contributes nothing and every run drains its worklist". The same artifacts show `z3` discoveries on 6 of 21 functions (see concolic-early-termination). Any benchmark or diagnosis that reads these fields from explore artifacts is measuring a constant.

## Evidence

- Audit run `ts-sub-concolic` (21 functions, `--concolic`): all 21 artifacts report `worklist_exhausted` and `solver_guided_inputs: 0`, including functions that ran exactly 100 iterations against a 100 budget and functions with 9 `z3` discoveries (classifyHttpResponse).
- `explore.rs:5238-5249` (resume path) and `:5824` (normal batch path) both route every observation through `accumulators[work_index].merge(...)`.
- No field in the struct at `explore.rs:190-211` carries either value.
- Tracker search (`bd search stop_reason`, `bd search solver_guided_inputs`, `bd search ExploreResultAccumulator`, 2026-09-23): no existing issue.

## Acceptance criteria

- [ ] The accumulator carries both fields with a documented merge policy in its doc comment:
  - `solver_guided_inputs` is summed across batches;
  - `stop_reason` is taken from the last successful batch (or a documented precedence), and a resumed-only function keeps its prior artifact's value.
- [ ] Unit tests on `ExploreResultAccumulator` merge two synthetic `ObservationOutput`s with non-default values (for example `MaxExecutions` and `solver_guided_inputs: 3` and `4`), and assert the merged `stop_reason` and `7`. Proof: the test fails on current main (paste the assertion output) and passes after the fix.
- [ ] A CLI-level test runs `shatter explore --concolic` on a fixture whose branch is only reachable with a solver-generated input (for example an equality against a constant), with a small `--max-iterations`. It asserts, from the written artifact JSON, that `solver_guided_inputs > 0` and that `stop_reason` is not the default when the budget was the limit. The same test with the default explorer asserts `stop_reason` equals the explorer's `classify_stop_reason` result.
- [ ] A grep over `shatter-cli/src` for `..Default::default()` in constructions of `ObservationOutput` finds no other construction that silently drops a field the engine sets. Any that remain are listed in the close note with the reason they are safe.
- [ ] Proof at close: the red and green test output, and the forced (uncached) `task e2e` output.

## Suggested approach

Add the two fields to the accumulator and its `merge`, and drop `..Default::default()` in `into_result` so the compiler flags any future `ObservationOutput` field the accumulator forgets. If a field is intentionally defaulted, name it explicitly.

## Out of scope

- Why concolic runs stop early (concolic-early-termination).
- The scan path's own artifact writer, unless the same defaulting is found there. If it is, list it in the close note and fix it here.

## Metadata

- Priority: P1 (blocks the D3 diagnosis and benchmark)
- Type: bug
- Labels: audit-2026-09-22, concolic, cli, artifacts, explore
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Blocks: concolic-early-termination, concolic-vs-default-benchmark
- Related: explore-resume-options-key
- Source findings: goals-08 (from Codex cross-check of draft concolic-early-termination)
- Decision refs: D3

---

---
slug: concolic-early-termination-fix
kind: new
title: "Fix the concolic early-termination defect identified by concolic-early-termination, with a known-answer E2E test under a bounded budget"
priority: P1
type: bug
labels: [audit-2026-09-22, concolic, orchestrator, solver]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [concolic-early-termination]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Fix the concolic early-termination defect identified by concolic-early-termination, with a known-answer E2E test under a bounded budget

## Problem

Maintainer decision D3 makes the concolic early-termination fix P1, so the default-vs-concolic benchmark measures a working engine. The diagnosis issue (concolic-early-termination) establishes what ends `--concolic` runs after 21-35 executions on functions such as computeArea, matchRoute and negotiateLanguage, and whether it is a defect. This issue implements the fix that diagnosis names.

If the diagnosis verdict is **expected** (every loss is accounted for by other issues, such as the TS instrumentation or Z3 sort-split bugs), close this issue with a link to that verdict. Do not invent a fix.

## Evidence

See concolic-early-termination for the per-function table from the audit run and the code sites. Scope, fixture and code sites for this issue come from that issue's close note.

## Acceptance criteria

- [ ] Known-answer E2E test in `shatter-core/tests/e2e_concolic.rs` (TS), driven through `pipeline_orchestrator` or the CLI entry point, not `orchestrator::explore` directly. The fixture is the one the diagnosis named. It has a branch that the pre-fix engine provably misses and that is reachable only by a solver-generated input. The test:
  - runs with a fixed seed and a bounded budget stated in the test (for example `max_iterations = 40`);
  - asserts that the target branch outcome is covered;
  - asserts that the covering input's discovery method is `z3` (or the solver provenance the diagnosis names), not `user_provided` or `fuzzed`;
  - asserts the artifact's `stop_reason` equals the expected reason for that budget, and `solver_guided_inputs >= 1`.

  It does not assert a minimum execution count: a correct solver may reach the target sooner.
- [ ] Proof: the commit SHA where the test fails on the pre-fix code, with the failing assertion output, and the passing output after the fix.
- [ ] If the diagnosis found the same defect reachable from Go or Rust, a matching case is added to `e2e_concolic_go.rs` or `e2e_concolic_rust.rs`, with the same red/green proof.
- [ ] A re-run of the diagnosis's 9-file subset, same command and examples SHA, attached to the close note as a before/after per-function table (`iterations`, `stop_reason`, `solver_guided_inputs`, branches). Every function whose branch coverage went down is explained.
- [ ] Forced (uncached) `task e2e` output at close.

## Out of scope

- Losses the diagnosis attributes to z3-mixed-int-real-sort-split, ts-switch-ternary-instrumentation or known-answer-ratchet-and-ts-discriminants. Those are fixed in their own issues.
- The benchmark and its post-fix run (concolic-vs-default-benchmark, concolic-benchmark-postfix-run).

## Metadata

- Priority: P1 (D3)
- Type: bug
- Labels: audit-2026-09-22, concolic, orchestrator, solver
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: concolic-early-termination
- Blocks: concolic-benchmark-postfix-run
- Related: explore-stop-reason-accounting, z3-mixed-int-real-sort-split, ts-switch-ternary-instrumentation
- Source findings: goals-08
- Decision refs: D3

---

---
slug: explore-budget-semantics
kind: new
title: "--max-iterations means a different budget per command and engine (concolic max_executions 1x in explore, 5x in scan/observe); give it one meaning and an explicit execution budget"
priority: P1
type: task
labels: [audit-2026-09-22, cli, explorer, orchestrator, parity]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# --max-iterations means a different budget per command and engine (concolic max_executions 1x in explore, 5x in scan/observe); give it one meaning and an explicit execution budget

## Problem

The help for `--max-iterations` on explore and scan says "Maximum number of iterations per function [default: 100]" (`shatter-cli/src/args.rs:505`, `:992`). observe says "(default: 50)" (`:1379`). What the flag actually bounds depends on the command and the engine:

- Under `--concolic`, `max_iterations` is a **unique-path** cap (`shatter-core/src/orchestrator.rs:1621-1627`), and a separate `max_executions` caps total executions (`:1628-1634`).
- `max_executions` is derived differently per command, and no flag sets it:
  - `explore`: `max_executions = max_iterations` (1×), `shatter-cli/src/commands/explore.rs:5145`;
  - `scan`: `concolic_scan_max_executions` = 5×, or 1× with custom generators (`shatter-core/src/scan_orchestrator.rs:3144-3150`);
  - `observe`: 5× (`shatter-cli/src/commands/observe.rs:109`).

The same `--max-iterations 100` therefore allows 100 executions in `explore --concolic` and 500 in `scan --concolic`. A default-vs-concolic comparison cannot set equal budgets (concolic-vs-default-benchmark, which is blocked on this issue).

This was part of the combined draft engine-path-identity-budget-config. It is split out because it is independently deliverable and on the D3 critical path.

## Acceptance criteria

- [ ] `--max-iterations` has one documented meaning across explore, scan, observe and run, for both engines. The help text on all four says exactly what it bounds (unique paths or executions) and gives the same default, or states why a command's default differs.
- [ ] One flag (for example `--max-executions`) sets the total-execution budget directly on explore, scan, observe and run, for both engines. Defined once in a shared options struct flattened into each command (the str-qwua7.20.1 direction).
- [ ] The derived default for `max_executions`, when the flag is absent, is computed by one function used by all commands. The 1× and 5× literals at `explore.rs:5145`, `scan_orchestrator.rs:3144-3150` and `observe.rs:109` are gone.
- [ ] Each run's effective budget (unique-path cap and execution cap) is written to the explore/scan artifact or summary, so a harness can read it back.
- [ ] Tests: for each of explore, scan, observe and run, and for both engines, a test asserts the effective budget built from the same flags is identical. A CLI test asserts that `--max-executions 30` stops a concolic run at 30 executions with `stop_reason: max_executions` (depends on explore-stop-reason-accounting for the explore path). Proof: the per-command test fails on current main (paste it) and passes after.
- [ ] SPEC.md's `--max-iterations` entry and the new flag are documented. The `task gauntlet` flag-permutation step exercises the new flag. Forced (uncached) `task e2e` and `task gauntlet` output at close.

## Out of scope

- Path identity (engine-path-identity-budget-config).
- The rest of the `orchestrator::ExploreConfig` unification (str-qwua7.6.2; qwua7-6-2-scan-observe-config-literals).

## Metadata

- Priority: P1 (blocks the D3 benchmark)
- Type: task
- Labels: audit-2026-09-22, cli, explorer, orchestrator, parity
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Blocks: concolic-vs-default-benchmark
- Related: str-qwua7.20.1 (shared ExploreOptions), str-qwua7.6.2, engine-path-identity-budget-config, explore-stop-reason-accounting
- Source findings: core-14 (split from draft shatter-code/17)
- Decision refs: D3

---

---
slug: concolic-fuzz-rng-unseeded
kind: new
title: "Concolic plateau fuzz phase uses StdRng::from_os_rng and ignores --seed, so seeded concolic runs are not reproducible"
priority: P1
type: bug
labels: [audit-2026-09-22, concolic, orchestrator, reproducibility, seeds]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Concolic plateau fuzz phase uses StdRng::from_os_rng and ignores --seed, so seeded concolic runs are not reproducible

## Problem

`scan --seed N` promises that "the same seed over unchanged source yields the same exploration" (`shatter-cli/src/args.rs:970-979`, str-0m0vn). The concolic orchestrator seeds its main RNG from the config (`shatter-core/src/orchestrator.rs:2561-2563`: `Some(seed) => StdRng::seed_from_u64(seed)`). But when the loop hits a coverage plateau and enters the fuzz phase, it builds a fresh RNG from OS entropy:

```rust
// shatter-core/src/orchestrator.rs:3060
let mut fuzz_rng = StdRng::from_os_rng();
```

Any seeded concolic run that reaches the fuzz phase is therefore not reproducible. The audit's concolic run entered that phase eight times across 21 functions. This blocks the D3 benchmark (concolic-vs-default-benchmark), which needs fixed seeds per arm.

## Evidence

- `orchestrator.rs:3060` is the only `from_os_rng` outside the `None` branch at `:2563` (`grep -n "from_os_rng" shatter-core/src/orchestrator.rs`, audit HEAD).
- Tracker searches on 2026-09-23 (`bd search from_os_rng`, `bd search "fuzz phase seed"`) found no existing issue. str-0m0vn (closed) widened `--seed` to the exploration RNG. str-pbqyr and str-9m9o3 cover the scan cache ignoring the seed, not this.

## Acceptance criteria

- [ ] When `config.seed` is `Some`, the fuzz-phase RNG is derived deterministically from it (for example `seed_from_u64(seed ^ FUZZ_STREAM)`, or drawn from the main seeded RNG). When it is `None`, behaviour is unchanged.
- [ ] Every other RNG construction in `shatter-core/src` non-test code is listed in the close note, each marked as seeded from config or intentionally entropy-based.
- [ ] Test: the orchestrator is run twice with the same seed on a fixture that reliably enters the fuzz phase (assert the phase was entered, for example via the fuzz execution counter). The generated fuzz inputs are identical across the two runs. A third run with a different seed is allowed to differ. Proof: the test fails on current main (paste the diff of inputs) and passes after the fix.
- [ ] Forced (uncached) `task e2e` output at close.

## Out of scope

- Adding `--seed` to explore and run (seed-for-explore-and-run).
- Scan cache keying on the seed (str-pbqyr, str-9m9o3).

## Metadata

- Priority: P1 (blocks the D3 benchmark)
- Type: bug
- Labels: audit-2026-09-22, concolic, orchestrator, reproducibility, seeds
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Blocks: concolic-vs-default-benchmark
- Related: seed-for-explore-and-run, str-0m0vn (closed), str-pbqyr, str-9m9o3
- Source findings: Codex cross-check of concolic-vs-default-benchmark (2026-09-23)
- Decision refs: D3

---

---
slug: concolic-benchmark-postfix-run
kind: new
title: "Run the default-vs-concolic benchmark after the early-termination fix and commit the results the positioning decision uses"
priority: P1
type: task
labels: [audit-2026-09-22, concolic, benchmark]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [concolic-vs-default-benchmark, concolic-early-termination-fix]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Run the default-vs-concolic benchmark after the early-termination fix and commit the results the positioning decision uses

## Problem

Maintainer decision D3 orders the concolic work as: build the benchmark, fix early termination, then decide the positioning from measured numbers. The benchmark harness (concolic-vs-default-benchmark) can close with a pre-fix result, and the fix (concolic-early-termination-fix) can close without re-running the whole benchmark. Without a separate issue, nobody owns the post-fix run that the decision (concolic-positioning-decision) needs. This issue owns it.

## Acceptance criteria

- [ ] The benchmark target from concolic-vs-default-benchmark is run on a `main` commit that contains both the harness and the early-termination fix (or the fix's "expected" closure). The run uses the checked-in manifest, all its seeds, and a forced, uncached invocation.
- [ ] The result file is committed under `benchmarks/baselines/explorers/` next to the pre-fix result, with its full provenance block (shatter commit, examples SHA, downstream commit, per-arm command lines, effective budget, seeds, host).
- [ ] The close note contains a short comparison against the pre-fix result: aggregate and per-stratum branch outcomes for each arm, and the functions whose concolic result changed by more than one branch outcome.
- [ ] The results are summarized in the release notes of the next release, and the close note links that entry.
- [ ] Proof at close: the command line and full output of the run, the committed file path and commit, and the release-notes link.

## Out of scope

- Changing the harness (reopen or follow up on concolic-vs-default-benchmark).
- The positioning decision (concolic-positioning-decision).

## Metadata

- Priority: P1 (D3 critical path)
- Type: task
- Labels: audit-2026-09-22, concolic, benchmark
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: concolic-vs-default-benchmark, concolic-early-termination-fix
- Blocks: concolic-positioning-decision
- Source findings: goals-08 (Codex cross-check: post-fix run had no owner)
- Decision refs: D3

---

---
slug: holdout-disposition
kind: new
title: "holdout reports impossible totals (294/294 branches, 401 fingerprint skips counted as errors) and has not run since April; archive or fix it"
priority: P3
type: task
labels: [audit-2026-09-22, effectiveness, benchmark, holdout]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [effectiveness-benchmark-holdout]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# holdout reports impossible totals (294/294 branches, 401 fingerprint skips counted as errors) and has not run since April; archive or fix it

## Problem

`~/project/holdout` is an earlier effectiveness harness. Its last run (`results/2026-04-09T08-34-54/summary.json`, shatter 120ef02a) reports numbers that cannot be real:

- `total_branches_covered 294` of `total_branches 294` (100%), while `targets_failed` is 7 of 12;
- `total_errors 401` equals `total_functions_skipped 401`: functions skipped as "unchanged (fingerprint match)" are counted as errors.

It has not run since April. Anyone reading its results gets a wrong picture of Shatter's effectiveness. This issue is filed in the shatter tracker because holdout's bd has no issues.

## Evidence (re-verified 2026-09-23)

```
$ python3 -c "import json;d=json.load(open('holdout/results/2026-04-09T08-34-54/summary.json')); print({k:v for k,v in d.items() if not isinstance(v,(list,dict))})"
{... 'targets_ok': 5, 'targets_failed': 7, 'total_functions_explored': 176, 'total_scope_functions': 577,
 'total_functions_skipped': 401, 'total_branches_covered': 294, 'total_branches': 294, 'total_behaviors': 11454, 'total_errors': 401}
```

## Acceptance criteria

Exactly one of these:

- [ ] **Archive:** holdout's README states at the top that it is retired, why (the two defects above), and points to the benchmark delivered by effectiveness-benchmark-holdout. Any scheduled or documented invocation of holdout, in shatter docs or elsewhere, is removed or redirected. Proof: the README diff and `grep -rn holdout` over shatter docs showing no live instructions.
- [ ] **Fix:** the branch total is computed from per-target data, so a run with failed targets cannot report 100%; fingerprint-match skips are counted as skips, not errors. A test over a synthetic summary with one failed target and one fingerprint skip asserts both. Proof: the test failing before and passing after, and one fresh run's `summary.json` with plausible totals.

The close note states which option was taken and why.

## Out of scope

- The new benchmark (effectiveness-benchmark-holdout).

## Metadata

- Priority: P3
- Type: task
- Labels: audit-2026-09-22, effectiveness, benchmark, holdout
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: effectiveness-benchmark-holdout (the archive option needs its replacement to point to)
- Source findings: goals-10 (split from draft other-first-party/50)

---

---
slug: effectiveness-repo-tracker-backlog
kind: new
title: "If the effectiveness benchmark lives in shatter-effectiveness: initialize a tracker there and file the remaining plan tasks"
priority: P3
type: task
labels: [audit-2026-09-22, effectiveness, tracker]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [effectiveness-benchmark-holdout]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# If the effectiveness benchmark lives in shatter-effectiveness: initialize a tracker there and file the remaining plan tasks

## Problem

`~/project/shatter-effectiveness` holds a 13-task implementation plan (`docs/superpowers/plans/2026-08-30-effectiveness-benchmark.md`) but no tracker, so the plan's remaining tasks are invisible to any issue-driven workflow. effectiveness-benchmark-holdout decides where the benchmark lives and delivers its first slice. If that decision is shatter-effectiveness, the remaining work needs a home there. If the decision is shatter, this issue is closed as not applicable with a link to the decision.

## Acceptance criteria

- [ ] If the location decision (recorded in effectiveness-benchmark-holdout) is **shatter**: this issue is closed as not applicable, linking the decision. Otherwise:
- [ ] A tracker (bd or GitHub Issues) is initialized in shatter-effectiveness, and its repo guidance says which tracker it uses.
- [ ] Each plan task not delivered by effectiveness-benchmark-holdout is filed there, one issue per task, each with acceptance criteria copied or adapted from the plan and a link back to the plan section. The close note lists the plan task numbers and the new issue IDs, and names every plan task deliberately not filed, with the reason.
- [ ] The delivered first slice is recorded there as done, with a link to its result file.

## Out of scope

- Implementing the plan tasks.
- holdout (holdout-disposition).

## Metadata

- Priority: P3
- Type: task
- Labels: audit-2026-09-22, effectiveness, tracker
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: effectiveness-benchmark-holdout
- Source findings: goals-10 (split from draft other-first-party/50)

---

---
slug: underscore-binding-lint
kind: new
title: "Lint _-prefixed parameters and let bindings in shatter-core/shatter-cli non-test code (they hid the concolic mock-variation regression)"
priority: P3
type: task
labels: [audit-2026-09-22, quality-gates, lint, shatter-core]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Lint _-prefixed parameters and let bindings in shatter-core/shatter-cli non-test code (they hid the concolic mock-variation regression)

## Problem

A leading `_` silences rustc's unused warning. In the orchestrator it hid a real regression: mock parameters were accepted and then dropped, so dynamic mock variation stopped working under `--concolic` (concolic-mock-variation-regression):

- `shatter-core/src/orchestrator.rs:2147`: `_mock_params: &[MockParam]`
- `orchestrator.rs:2643`: `let _initial_mocks = ...`

Nothing flags a new `_`-prefixed binding, so the next dropped input will be just as silent. This was part of the combined draft engine-parity-e2e and is split out as a separate deliverable.

## Evidence

A rough grep at audit HEAD finds about 92 `_`-prefixed `let` and parameter bindings across `shatter-core/src` and `shatter-cli/src`, including test modules (same-runtime cross-check count). The non-test share has not been measured.

## Acceptance criteria

- [ ] A script under `scripts/`, wired into `task check-static`, flags `_`-prefixed function parameters and `let` bindings in `shatter-core/src` and `shatter-cli/src`, excluding:
  - `#[cfg(test)]` modules and `tests/` directories;
  - bare `_` and `let _ = ...` (explicit discard);
  - bindings with a `// allow-underscore: <reason>` or `TODO(str-...)` comment on the same or previous line.

  The exclusion rules are written in the script's header.
- [ ] Before wiring, the close note records the number of existing non-test hits. Each is fixed (binding used or removed) or annotated with a reason or issue ID. No blanket allowlist file.
- [ ] Proof at close: the script run on a branch with a planted `fn f(_unused: u32)` in `shatter-core/src` (fails, naming the file and line); the script passing on the final branch; the `task check-static` output showing the step ran uncached.

## Out of scope

- Fixing the mock-variation regression (concolic-mock-variation-regression).
- Other crates.

## Metadata

- Priority: P3
- Type: task
- Labels: audit-2026-09-22, quality-gates, lint, shatter-core
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: concolic-mock-variation-regression, engine-parity-e2e
- Source findings: core-22 (split from draft shatter-agent/23)

---

---
slug: pipeline-close-reason-rule
kind: new
title: "CLAUDE.md completion checklist: pipeline fixes must name the production call site and the pipeline-level test; test workarounds in prose must be filed"
priority: P3
type: task
labels: [audit-2026-09-22, docs, agent-guidance, testing]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# CLAUDE.md completion checklist: pipeline fixes must name the production call site and the pipeline-level test; test workarounds in prose must be filed

## Problem

Two issues were closed on evidence from non-production paths: str-0s76.6, via a test that calls `orchestrator::explore` directly while production callers pass `None` for the setup context, and str-55ep, which fixed a dead shrinker copy. The CLAUDE.md Completion Checklist (`CLAUDE.md:43` onward) accepts unit and API tests as proof of pipeline behaviour for anything except the named E2E commands. It never asks which production caller was exercised.

Test workarounds are also recorded only as prose. For example `shatter-ts/CLAUDE.md:283-287` says the E2E reads `raw_results` because switch emits no `branch_path`. That is a product gap (ts-switch-ternary-instrumentation) described as a testing note.

This was part of the combined draft engine-parity-e2e and is split out as a separate deliverable.

## Acceptance criteria

- [ ] The CLAUDE.md Completion Checklist gains a rule: the close reason for a pipeline feature or fix names (a) the production call site exercised, as `file:line` of the caller, and (b) the pipeline-level test that proves it, which must go through `pipeline_orchestrator`/`run_pipeline` or the CLI, not `orchestrator::explore` or `explorer::explore_function`.
- [ ] The same checklist gains a rule: a test workaround that exists because of a product gap is filed as an issue, and the prose links that issue ID.
- [ ] A grep over every `CLAUDE.md` in the repo for workaround phrasing ("because", "workaround", "instead of", "reads raw_results") is reviewed. Each product-gap workaround found is linked to an existing issue or a new one. The list goes in the close note.
- [ ] The rule is cross-linked with bento's close-reason-evidence draft (bento bucket), so the generic bento rule and this repo rule agree. If that draft is not filed yet, the close note says so.
- [ ] Proof at close: the CLAUDE.md diff and the grep review list.

## Out of scope

- The engine_parity suite (engine-parity-e2e).
- Changing bento's skill text (close-reason-evidence, bento bucket).

## Metadata

- Priority: P3
- Type: task
- Labels: audit-2026-09-22, docs, agent-guidance, testing
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: engine-parity-e2e, ts-switch-ternary-instrumentation, close-reason-evidence (bento); bento-m4en, bento-a0nz; str-0s76.6, str-55ep
- Source findings: core-22 (split from draft shatter-agent/23)

---

---
slug: core-reachability-gate
kind: new
title: "Add a production-reachability check for shatter-core pub items (defined roots and graph traversal), wired into check-static"
priority: P3
type: task
labels: [audit-2026-09-22, quality-gates, shatter-core, tech-debt]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [core-dead-code-removal]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Add a production-reachability check for shatter-core pub items (defined roots and graph traversal), wired into check-static

## Problem

`pub` items in a library crate never trigger rustc's dead-code warning, and cargo-machete covers only dependencies. That is how about 3,400 lines of shatter-core became production-dead without anyone noticing (core-dead-code-removal). A check is needed so it does not recur.

A simple "referenced from outside its own file" count is not enough:

- Two dead modules that reference each other both pass (for example `reporter.rs` and `clustering.rs` today).
- A live pub helper that is only called from within its own file, by a function that is itself reachable, fails.

This was part of the combined draft core-dead-code-removal and is split out as a separate deliverable.

## Acceptance criteria

- [ ] The check's model is written in the script header:
  - **Roots:** the `main` functions of the workspace binaries (`shatter-cli` and any other `[[bin]]`), plus pub items used by non-test code of other workspace crates (the frontends, shatter-llm). Test code, `tests/`, `benches/` and examples are not roots.
  - **Traversal:** reachability over a reference graph of items (functions, methods, types, modules), built from rustdoc JSON (`cargo +nightly rustdoc --output-format json`) or an equivalent item-level source.
  - **Report:** every pub module and pub fn in shatter-core not reachable from a root.
- [ ] If an item-level graph is not practical, the script may use a documented heuristic instead. It must then carry regression cases for both limits above (mutually referencing dead modules; a live helper used only in its own file) and state in the header which of them it gets wrong.
- [ ] An allowlist file with one reason per entry. The check fails on any new unreachable item not in the allowlist, and on any allowlisted item that has become reachable (stale entry).
- [ ] The script is wired into `task check-static`.
- [ ] Regression fixtures (a small test crate or synthetic graph input) cover:
  - an unreachable pub fn (reported);
  - two mutually referencing unreachable modules (both reported);
  - a pub helper called only within its own file from a reachable fn (not reported);
  - a pub fn used only by tests (reported).
- [ ] Proof at close: the regression fixture output; the script run directly (not through the cached task) on a branch with a planted unused pub fn in shatter-core (fails, naming it); the script passing on the final branch; the allowlist contents.

## Out of scope

- Deleting the currently dead code (core-dead-code-removal, which lands first so the initial allowlist is small).
- Unused-code tooling in the bento audit skill (bento-r85d).
- Module-cycle checks (str-qwua7.29).

## Metadata

- Priority: P3
- Type: task
- Labels: audit-2026-09-22, quality-gates, shatter-core, tech-debt
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: core-dead-code-removal
- Related: str-qwua7.29, bento-r85d, engine-parity-e2e
- Source findings: core-08 (split from draft shatter-code/18)

---

---
slug: qwua7-6-2-scan-observe-config-literals
kind: note-to-existing
title: "Note on str-qwua7.6.2: scan and observe also hand-build orchestrator::ExploreConfig, and the three literals already diverge"
priority: P1
type: note
labels: [audit-2026-09-22, architecture, parity, orchestrator]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.6.2
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.6.2: scan and observe also hand-build orchestrator::ExploreConfig, and the three literals already diverge

**Target:** `str-qwua7.6.2` (open P1, "Unify explorer::ExploreConfig and orchestrator::ExploreConfig behind a shared base"; verified with `bd show` on 2026-09-23). Action: append the comment below. Do not create a new issue. Priority unchanged.

## Comment text

Audit 2026-09-22 note (finding core-14; evidence `audits/2026-09-22/areas/core-engine.md`).

This issue names only the CLI explore translation (`explore.rs:5138-5171`). Two more non-test sites hand-build `orchestrator::ExploreConfig`:

- `shatter-cli/src/commands/explore.rs:5138-5169`
- `shatter-core/src/scan_orchestrator.rs:3080-3102`
- `shatter-cli/src/commands/observe.rs:107-130`

(`pipeline_orchestrator.rs:1342` is inside `mod tests`.)

They already disagree:

| Field | explore | scan | observe |
|---|---|---|---|
| `seed` | None | `explore_config.seed` | None |
| `refine_budget` | set | None | None |
| `default_execute_plan` | None | threaded | None |
| `mcdc` | flag | false | false |
| `fuzz` | resolved | default | default |
| `mocks` / `mock_params` | from config | from config | empty |
| `solver_timeout_ms` | from flag | from flag | None |
| `plateau_threshold` | 20 or 60 | 20 | 20 |

**Proposed additions to this issue's acceptance:**

- [ ] The shared constructor (`From<&ExploreConfig>` or equivalent) is used by explore, scan and observe. `grep -n "orchestrator::ExploreConfig {"` over non-test code in `shatter-cli/src` and `shatter-core/src` finds no struct literal.
- [ ] Each field in the table above either comes from the shared base or is set by a named, per-command override with a comment giving the reason. Unit tests assert that the shared fields round-trip for all three commands.
- [ ] The `max_executions` derivation is out of scope here; explore-budget-semantics (audit 2026-09-22) owns it. Coordinate so the constructor calls that issue's single budget function.
