# Bundle: shatter-concolic-and-engine-design

- **Bucket:** shatter-concolic-and-engine-design
- **Repo / tracker:** shatter, bd in /home/ketan/project/shatter (prefix str)
- **Parent epic:** Epic: Audit 2026-09-22 findings
- **Theme:** Measure concolic before positioning it (D3), engine parity, path/budget semantics, dead code, and effectiveness measurement.
- **Status:** drafts only; nothing filed (D6).
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

Decisions touching this bucket: **D3** (drafts 01-03). D6 applies to all.

## Contents

| # | Slug | Kind | Priority | Title |
|---|---|---|---|---|
| 01 | concolic-vs-default-benchmark | new | P1 | Add a controlled default-vs-concolic coverage benchmark (fixed seeds, fresh artifacts, examples corpus + one downstream project), published per release |
| 02 | concolic-early-termination | new | P1 | Concolic explorer stops after ~21-35 executions with zero solver-guided inputs on most hard TS functions; find root cause and fix |
| 03 | concolic-positioning-decision | new | P2 | Decision: keep or revise the 'concolic-first' product positioning in README/SPEC, using the default-vs-concolic benchmark numbers |
| 04 | effectiveness-benchmark-holdout | new | P2 | Deliver a minimal bug-finding effectiveness benchmark (known-answer + downstream subset) and retire or fix holdout |
| 05 | engine-path-identity-budget-config | new | P2 | Engines disagree on what a 'path' is, --max-iterations means different budgets per entry point, and three hand-built orchestrator::ExploreConfig literals diverge |
| 06 | engine-parity-e2e | new | P2 | Replace prose-only random-vs-concolic parity with gates: engine_parity E2E suite through the pipeline, `_`-param lint, close-reason call-site rule |
| 07 | core-dead-code-removal | new | P2 | Remove ~3,400 lines of production-dead shatter-core code (recursive, array_mutation, reporter+clustering, sequential scan(), shrink_witness) and add a reachability gate |
| 08 | stable-hash-persisted-keys | new | P3 | std DefaultHasher keys persisted/reproducible values (core_sample --seed selection, Rust harness cache dirs); use a specified hash |
| 09 | qwua7-6-function-length-ratchet | note-to-existing (str-qwua7.6) | P3 | Note on str-qwua7.6: explore_with_oracle grew 1,307 -> 1,372 lines; add a function-length ratchet |
| 10 | qwua7-43-bench-dev-dep-cycle | note-to-existing (str-qwua7.43) | P2 | Note on str-qwua7.43: bench_frontier_ranking.rs deepened the core -> shatter-llm dev-dependency cycle |

---

<!-- file: 01-concolic-vs-default-benchmark.md -->

---
slug: concolic-vs-default-benchmark
kind: new
title: "Add a controlled default-vs-concolic coverage benchmark (fixed seeds, fresh artifacts, examples corpus + one downstream project), published per release"
priority: P1
type: task
labels: [audit-2026-09-22, concolic, benchmark, effectiveness]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Add a controlled default-vs-concolic coverage benchmark (fixed seeds, fresh artifacts, examples corpus + one downstream project), published per release

## Problem

README.md:1-3, SPEC.md, CLAUDE.md:3 and the CLI `--help` about text (`shatter-cli/src/args.rs:117`) describe Shatter as "automatic exploratory testing via concolic execution". The default explorer, however, is the adaptive random/hybrid scheduler (`explorer.rs`). The Z3-driven concolic orchestrator (`orchestrator.rs`) runs only with `--concolic` (SPEC.md:148, 713-715). Nobody has measured whether `--concolic` finds more than the default:

- The audit's one fresh concolic run (21 hard TS functions) scored 164/454 lines (36.1%). The audit report compared this with a default number of 185/454 (40.7%), but the verifier found no default-subset artifact for those 21 functions, so **the default baseline is unverified and must be re-measured**.
- Kapow agent memory (2026-07-02) records "--concolic ... ZERO coverage delta vs random".
- str-ior1 ("re-baseline Zolem with --concolic") was closed with the reason "Closed" and no data.

Maintainer decision D3 (2026-09-23): **measure first**. This issue delivers the measurement. A separate decision issue (concolic-positioning-decision) re-decides the README/SPEC positioning from these numbers. Do not change positioning docs here.

## Evidence

- Concolic run: `audits/2026-09-22/goals-runs/ts-sub-concolic.{err,md}` (fresh directory; 9 files, 21 functions, `-w 4`). Artifacts are in `goals-runs/ts-concolic-fresh/shatter-artifacts/explore-results/`.
- The claimed default comparison (`areas/goals.md:90-99`): classifyHttpResponse 89% -> 59%, matchRoute 12% -> 4%, authorizeRequest 14% -> 21%. No matching default artifact exists under `goals-runs/` (verifier note on goals-08).
- Line totals from the markdown reports are themselves suspect: the random explorer under-counts paths from the float-probe phase (float-probe-paths-uncounted, bucket shatter-engine-correctness). Use branch outcomes as the primary metric and lines as secondary.
- Resume contamination: a later run in the same artifact dir silently re-emits prior results regardless of explorer mode (`areas/artifacts.md:105-113`; explore-resume-options-key). One such replay printed `[resumed] classifyNumber: 3 branches, 7.6s (prior run)` under a `--concolic` step.
- Seeds: `--seed` exists only on `scan` (`shatter-cli/src/args.rs:970-979`), not on `explore` (seed-for-explore-and-run). The concolic config built by `explore` hard-codes `seed: None` (`shatter-cli/src/commands/explore.rs:5150`).
- Budgets differ by entry point: `explore --concolic` sets `max_executions = max_iterations` (`explore.rs:5145`), while scan uses 5x (`scan_orchestrator.rs:3144-3150`). An A/B that mixes entry points is not controlled (engine-path-identity-budget-config).
- Reusable pieces:
  - `benchmarks/frontier-ranking/manifest.json` already defines seeds, regimes and fixtures with strata.
  - `task bench-frontier` / `bench-frontier-report` (Taskfile.yml:845-876) and `scripts/bench_frontier_report.py` already run and report such benchmarks.
  - Unlike those, this benchmark calls `orchestrator::explore_with_oracle` directly (`shatter-core/tests/bench_frontier_ranking.rs:381`), so it bypasses the CLI wiring that differs between modes.
- Examples corpus: the external checkout resolved by `scripts/examples_checkout.py` (unpinned `origin/main`; see pin-examples-repo).

## Acceptance criteria

- [ ] A `task bench-explorers` target (name is the implementer's choice) runs the **same** function set under the default explorer and under `--concolic`. Requirements:
  - it goes through the user-facing CLI entry point, not `orchestrator::` directly;
  - it uses the same entry point for both arms and a documented, equal execution budget;
  - it uses fixed seeds, at least 3 per arm;
  - each arm × seed gets a fresh artifact directory (plus `--clean`), and the run fails if any stderr contains `[resumed]`.
- [ ] Corpus: all TS/Go/Rust examples-corpus functions that have `EXPECTED BRANCHES` comments, run at a pinned examples SHA that is recorded in the output, plus one named subset of one downstream project (kapow, zolem or pickpackit). The subset is listed in a manifest file.
- [ ] Per function and in aggregate, the output reports:
  - branch outcomes covered and known-answer expected outcomes hit;
  - lines covered, labelled secondary;
  - executions used and stop reason;
  - wall time;
  - mean and spread across seeds.
- [ ] The default-explorer baseline for the audit's 21-function subset (see `goals-runs/ts-sub-concolic.md`) is re-measured, and the 40.7% figure is confirmed or replaced in this issue's close note.
- [ ] Results are committed under `benchmarks/baselines/explorers/<version>.json` (or similar) and summarized in the release notes. The release checklist (RELEASE docs or release workflow) gains a step to run the benchmark per release.
- [ ] Proof at close: the command line and full output of one forced, uncached run on a clean checkout, the committed results file, and a link to the release-notes entry for the first release that carries the numbers.

## Suggested approach

1. Reuse the frontier-ranking manifest format and `bench_frontier_report.py` style. Drive `shatter scan --seed N [--concolic]` (the only entry point with `--seed` today), or `explore` once seed-for-explore-and-run lands.
2. Pin budgets explicitly (`--max-iterations`, plus the effective `max_executions`) and record them in the output. The budget mismatch is itself a finding for engine-path-identity-budget-config.
3. Where explorers differ on the same function, keep per-function rows, so that regressions such as classifyHttpResponse 13/13 -> 10/13 stay visible.
4. The benchmark can share a harness with effectiveness-benchmark-holdout, but it measures coverage per explorer, not bug-finding.

## Out of scope

- Fixing concolic early termination (concolic-early-termination).
- Changing README/SPEC/CLAUDE.md positioning (concolic-positioning-decision, D3).
- Fixing the budget/config divergence itself (engine-path-identity-budget-config).
- Resume keying (explore-resume-options-key).

## Metadata

- Priority: P1 (raised from P2 by D3)
- Type: task
- Labels: audit-2026-09-22, concolic, benchmark, effectiveness
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none (fresh dirs and `scan --seed` avoid hard dependencies)
- Related: concolic-early-termination, concolic-positioning-decision (blocked by this issue), effectiveness-benchmark-holdout, explore-resume-options-key, seed-for-explore-and-run, engine-path-identity-budget-config, pin-examples-repo, float-probe-paths-uncounted; str-ior1 (closed without data), str-2fui, str-qwua7.6
- Source findings: goals-08 (draft shatter-code/80)
- Decision refs: D3

---

<!-- file: 02-concolic-early-termination.md -->

---
slug: concolic-early-termination
kind: new
title: "Concolic explorer stops after ~21-35 executions with zero solver-guided inputs on most hard TS functions; find root cause and fix"
priority: P1
type: bug
labels: [audit-2026-09-22, concolic, orchestrator, solver]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Concolic explorer stops after ~21-35 executions with zero solver-guided inputs on most hard TS functions; find root cause and fix

## Problem

On the audit's fresh `--concolic` run over 21 hard TypeScript example functions, most functions stopped well short of the 100-iteration budget and left most branches uncovered. Every one of the 21 artifacts reports `solver_guided_inputs: 0`. The Z3 loop that `--concolic` exists for appears to contribute nothing: the worklist drains after the seed inputs (and an occasional fuzz phase). Until this is fixed, any concolic-vs-default comparison (concolic-vs-default-benchmark) measures a broken engine.

## Evidence

Run: `audits/2026-09-22/goals-runs/ts-sub-concolic.err` (fresh directory, `-w 4`). Artifacts: `goals-runs/ts-concolic-fresh/shatter-artifacts/explore-results/`.

| Function | File | Iters | Paths | Branches |
|---|---|---|---|---|
| computeArea | 05-unions.ts | 21 | 1 | 0/6 |
| matchRoute | 10-path-router.ts | 21 | 1 | 1/19 |
| classifyStatus | 17-mock-branches.ts | 21 | 1 | 0/3 |
| loadOrDefault | 17-mock-branches.ts | 21 | 1 | 1/2 |
| classifyConfigs | 17-mock-branches.ts | 22 | 2 | 1/4 |
| negotiateLanguage | 18-accept-language.ts | 23 | 2 | 1/12 |
| routeRequest | 05-unions.ts | 26 | 2 | 1/8 |
| authorizeRequest | 07-auth-validation.ts | 29 | 3 | 3/14 |
| classifyHttpResponse | 06-nested-control-flow.ts | 35 | 6 | 10/13 |

Tabulated from the artifact JSON with:

```
cd audits/2026-09-22/goals-runs/ts-concolic-fresh/shatter-artifacts/explore-results
python3 -c "import json,glob
for f in sorted(glob.glob('*/0*.json')):
  o=json.load(open(f))['observation']
  print(f, o['iterations'], o['stop_reason'], o['solver_guided_inputs'], len(o['raw_results']))"
```

- All 21 functions report `stop_reason: worklist_exhausted` and `solver_guided_inputs: 0`. That includes functions that ran exactly 100 iterations (processStateMachine, validateJwt, parseSemver, validateEmail, parsePreference, parseDotenv). This suggests that either the stop-reason mapping is wrong, or those runs also ended by draining the worklist at the budget boundary.
- The stderr shows `Coverage plateau — entering fuzz phase targeting N opaque branch(es)` eight times (lines 9, 14, 19, 33, 48, 62, 64, 76).
- The artifact's `solver_guided_inputs` is `r.z3_generated + r.boundary_generated + r.drill_generated` (`shatter-core/src/pipeline.rs:951`), so Z3, boundary and drilling each produced 0 follow-ups, or the counters are not incremented on this path.

### Relevant code at audit HEAD (56c86168)

- Termination checks in `observe_one`: `shatter-core/src/orchestrator.rs:1621-1646`. They cover max_iterations (a unique-path cap), max_executions, timeout, and `plateau_threshold` (default 20, `orchestrator.rs:203`). The CLI sets 20 (or 60 with `--mcdc`) at `shatter-cli/src/commands/explore.rs:5146`, `scan_orchestrator.rs:3083` and `observe.rs:110`.
- The loop's default termination is `WorklistExhausted` (`orchestrator.rs:2557`). The CoveragePlateau handler enters the fuzz phase or breaks (`orchestrator.rs:2972-3180`).
- The "21 = 1 + plateau_threshold 20" pattern and the "21 = seed-set size" pattern are both consistent with the data. Which one applies is the first thing to establish.

### Likely contributors (link, do not duplicate)

- z3-mixed-int-real-sort-split (bucket shatter-engine-correctness): one parameter becomes two unrelated Z3 variables, and model extraction overwrites values across sorts, so SAT models can produce no usable input.
- ts-switch-ternary-instrumentation (bucket shatter-frontend-ts): switch/ternary/value-position `&&`/`||` are analyzed but emit no `branch_path` decisions, so there is nothing to negate on those branches.
- known-answer-ratchet-and-ts-discriminants: computeArea's discriminant literal is widened to `str`, and the generated `{"kind":"true",...}` matches no case (`areas/goals.md`).

## Acceptance criteria

- [ ] Root cause documented in this issue. For at least computeArea, matchRoute and negotiateLanguage, the note says which of these ends the loop: worklist exhaustion, plateau, or unsat/unknown/solver-error handling. It also says why no solver-guided input is produced (no path constraints emitted, Z3 unsat/unknown, model extraction dropped values, or counters not incremented).
- [ ] `stop_reason` and `solver_guided_inputs` in the explore artifact are accurate for the concolic path. A unit test covers each `TerminationReason` -> `StopReason` mapping, including a run that hits `max_iterations`.
- [ ] Fix landed. On a fresh run of the same 21 functions, no function whose branches are not all covered stops before its budget with `worklist_exhausted` unless the issue documents why the worklist is empty. `solver_guided_inputs > 0` on functions with solvable numeric/string branches (e.g. matchRoute, negotiateLanguage).
- [ ] Known-answer E2E test (in `shatter-core/tests/e2e_concolic.rs`, driven through `pipeline_orchestrator`/CLI wiring, not `orchestrator::explore` directly). It uses a fixture with a branch reachable only via a solver-generated input, and asserts:
  - the run exceeds the old ~21-execution stop point;
  - the branch is reached with a solver provenance.

  Proof at close: the commit SHA where the test fails on the pre-fix code, the passing run output after the fix, and the command/output of a forced (uncached) `task e2e` run.
- [ ] Re-run of the 21-function subset attached to the close note (per-function iters / branches / stop_reason / solver_guided_inputs, before and after).
- [ ] Losses traced to z3-mixed-int-real-sort-split or ts-switch-ternary-instrumentation are listed with those issue IDs rather than fixed here.

## Suggested approach

1. Reproduce on one function (computeArea or matchRoute) with `RUST_LOG=debug` in a fresh artifact dir. Log each worklist push/pop with its source (seed, z3, boundary, drill, fuzz) and each solver call result (sat/unsat/unknown/error).
2. Check whether branch decisions for these functions carry `SymExpr` path conditions at all (the TS instrumentor may emit decisions without symbolic constraints for object-field or string ops). If the conditions are there, check the Z3 results.
3. Fix the accounting (`stop_reason`, `solver_guided_inputs`) first, so later runs are self-explaining.

## Out of scope

- The benchmark itself (concolic-vs-default-benchmark).
- The positioning decision (concolic-positioning-decision).
- Path-identity or budget-semantics unification (engine-path-identity-budget-config).
- The fixes in the linked sort-split and TS-instrumentation issues.

## Metadata

- Priority: P1 (D3)
- Type: bug
- Labels: audit-2026-09-22, concolic, orchestrator, solver
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: z3-mixed-int-real-sort-split, ts-switch-ternary-instrumentation, known-answer-ratchet-and-ts-discriminants, concolic-vs-default-benchmark; blocks concolic-positioning-decision
- Source findings: goals-08 (split from draft shatter-code/80)
- Decision refs: D3

---

<!-- file: 03-concolic-positioning-decision.md -->

---
slug: concolic-positioning-decision
kind: new
title: "Decision: keep or revise the 'concolic-first' product positioning in README/SPEC, using the default-vs-concolic benchmark numbers"
priority: P2
type: decision
labels: [audit-2026-09-22, concolic, docs, decision]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [concolic-vs-default-benchmark, concolic-early-termination]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Decision: keep or revise the 'concolic-first' product positioning in README/SPEC, using the default-vs-concolic benchmark numbers

## Problem

Shatter presents itself as concolic-first, but the default explorer is not the concolic engine, and the one audit measurement of `--concolic` was uncontrolled (goals-08). Maintainer decision D3 (2026-09-23) is to measure first and then decide. This issue holds that decision. It is intentionally blocked until the measurement exists and the known concolic early-termination bug is fixed, so the decision rests on a working engine and controlled numbers.

## Evidence (current positioning, audit HEAD 56c86168)

- `README.md:3`: "Automatic exploratory testing via concolic execution."
- `CLAUDE.md:3`: the same sentence.
- `shatter-cli/src/args.rs:117`: CLI about text, "Shatter: automatic exploratory testing via concolic execution."
- `SPEC.md:148`: `--concolic` (default false) "Use the concolic (Z3-backed) explorer instead of the random explorer."
- `SPEC.md:703-716`: the default adaptive scheduler blends user, boundary, solver-guided and random inputs; `--concolic` is listed as an alternative explorer.
- Audit run: `goals-runs/ts-sub-concolic.md`, 36.1% lines on 21 functions. The default baseline of 40.7% was not reproduced (verifier note on goals-08).

## Acceptance criteria

- [ ] Decision recorded in this issue, citing the committed benchmark results from concolic-vs-default-benchmark, measured after concolic-early-termination landed. The decision is one of:
  - (a) keep "concolic-first" (concolic measurably wins or ties on the stated metric);
  - (b) reposition (for example "solver-assisted exploratory testing", with `--concolic` as an option);
  - (c) make `--concolic` the default.

  Each option names the metric and threshold it relies on.
- [ ] If (b) or (c): follow-up issue(s) filed for the README/SPEC/CLAUDE.md/`--help` text and for any default change. Each names the exact files and lines, and they are linked here.
- [ ] If (a): this issue states which benchmark result justifies the wording, and the per-release benchmark keeps checking it. A regression reopens the decision.
- [ ] Proof at close: link to the benchmark results file and release-notes entry the decision used.

## Suggested approach

Review the per-function rows as well as the aggregate. Mixed results (for example validateEmail 6/19 -> 13/19 vs classifyHttpResponse 13/13 -> 10/13 in the audit run) may favor a hybrid positioning over a binary answer.

## Out of scope

- Editing README/SPEC now. D3 forbids doc softening before the measurement.
- Running the benchmark.
- Fixing the engine.

## Metadata

- Priority: P2
- Type: decision
- Labels: audit-2026-09-22, concolic, docs, decision
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: concolic-vs-default-benchmark, concolic-early-termination
- Related: str-ior1 (closed without data)
- Source findings: goals-08
- Decision refs: D3

---

<!-- file: 04-effectiveness-benchmark-holdout.md -->

---
slug: effectiveness-benchmark-holdout
kind: new
title: "Deliver a minimal bug-finding effectiveness benchmark (known-answer + downstream subset) and retire or fix holdout"
priority: P2
type: task
labels: [audit-2026-09-22, effectiveness, benchmark]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Deliver a minimal bug-finding effectiveness benchmark (known-answer + downstream subset) and retire or fix holdout

## Problem

Shatter's effectiveness measurement ("does it find real bugs and behaviours?") has been designed three times and never delivered, so no one can tell whether engine changes improve results. This issue lives in the shatter tracker because `~/project/shatter-effectiveness` has no tracker and holdout's bd has no issues.

- **holdout** (`~/project/holdout`): the last run (`results/2026-04-09T08-34-54/summary.json`, shatter 120ef02a) reports `targets_ok 5`, `targets_failed 7` and `total_branches_covered 294` of `total_branches 294`, which cannot be real. `total_errors 401` equals `total_functions_skipped 401`: fingerprint-match cache skips ("unchanged (fingerprint match)") are counted as errors. It has not run since April.
- **shatter-effectiveness** (`~/project/shatter-effectiveness`): it contains a 1,057-line design (`docs/specs/2026-08-27-effectiveness-benchmark-design.md`) and a 1,260-line 13-task plan (`docs/superpowers/plans/2026-08-30-effectiveness-benchmark.md`). It holds only `docs/ scripts/ tests/` (a docs-integrity gate) and no `bench/` code. Last commit: 6e234fe, 2026-08-31, "Merge implementation-plan".
- Downstream ≥90% coverage goals (kapow, zolem, pickpackit) have stalled at 18-28% since 2026-07-07 with no metric linking engine work to outcomes (see downstream-coverage-goals-epic).

This is distinct from concolic-vs-default-benchmark, which measures **coverage per explorer**. This issue measures **bug/behaviour-finding effectiveness**. The two may share a harness (corpus checkout, manifest format, run/record scripts).

## Evidence (re-verified 2026-09-23)

```
$ python3 -c "import json;d=json.load(open('holdout/results/2026-04-09T08-34-54/summary.json')); print({k:v for k,v in d.items() if not isinstance(v,(list,dict))})"
{... 'targets_ok': 5, 'targets_failed': 7, 'total_functions_explored': 176, 'total_scope_functions': 577,
 'total_functions_skipped': 401, 'total_branches_covered': 294, 'total_branches': 294, 'total_behaviors': 11454, 'total_errors': 401}
$ git -C shatter-effectiveness log -1 --format='%h %ad %s' --date=short
6e234fe 2026-08-31 Merge implementation-plan: thirteen-task plan for the benchmark
$ ls shatter-effectiveness
docs  scripts  tests
```

## Acceptance criteria

- [ ] Location decision recorded in this issue: shatter-effectiveness, or a `task` target inside shatter. Reasons: tracker, CI access, corpus pinning.
- [ ] A minimal on-demand benchmark exists and records a dated result file per run. It is either:
  - the smallest slice of the effectiveness plan (Task 1 probes plus Tasks 4-5 distill/score over one target), or
  - a known-answer benchmark built from the examples' `EXPECTED BRANCHES` comments plus a seeded-bug subset of one downstream project.

  The metric is bug/behaviour-finding (for example seeded faults detected and expected outcomes found), not raw line coverage.
- [ ] holdout is either archived (README note pointing to the new benchmark) or fixed: the branch totals must be real, and fingerprint-match skips must be counted as skips, not errors.
- [ ] If the harness is shared with concolic-vs-default-benchmark, the shared parts are named in both issues.
- [ ] Proof at close: the command and output of one uncached run, plus the committed or published dated result file.
- [ ] If the location is shatter-effectiveness, a tracker (bd or GitHub Issues) is initialized there and the remaining plan tasks are filed there. This issue closes with links to them.

## Suggested approach

Make the location decision first, then split: (1) the first slice of the benchmark, (2) the holdout disposition. Reuse `scripts/examples_checkout.py` and a pinned examples SHA (pin-examples-repo).

## Out of scope

- The full 13-task plan.
- Coverage comparison between explorers (concolic-vs-default-benchmark).
- Downstream coverage-goal work itself.

## Metadata

- Priority: P2
- Type: task
- Labels: audit-2026-09-22, effectiveness, benchmark
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: concolic-vs-default-benchmark (possible shared harness, different metric), downstream-coverage-goals-epic, pin-examples-repo; kapow-94wr (earlier "eval harness unreliable" symptom); str-jeen.14 (closed, broad-run validation corpus)
- Source findings: goals-10 (draft other-first-party/50)

---

<!-- file: 05-engine-path-identity-budget-config.md -->

---
slug: engine-path-identity-budget-config
kind: new
title: "Engines disagree on what a 'path' is, --max-iterations means different budgets per entry point, and three hand-built orchestrator::ExploreConfig literals diverge"
priority: P2
type: task
labels: [audit-2026-09-22, architecture, parity, explorer, orchestrator]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Engines disagree on what a 'path' is, --max-iterations means different budgets per entry point, and three hand-built orchestrator::ExploreConfig literals diverge

## Problem

1. **Path identity.** The random explorer and the concolic orchestrator hash "paths" differently. On a loop function with two behaviours, concolic reports 16 paths and the random explorer 6. The random explorer's own shrink-witness selection uses the concolic hash rather than its path hash, so one engine even uses two identities.
2. **Budget semantics.** Concolic `max_iterations` is a unique-path cap, while `max_executions` is set to 1x `--max-iterations` in `explore` and 5x in `scan` and `observe`. The help text for all of them says "Maximum number of iterations per function".
3. **Config construction.** `explore`, `scan` and `observe` each build `orchestrator::ExploreConfig` by hand, with different seeds, budgets, refine settings, execute plans, mocks and solver timeouts.

Users therefore cannot compare results across commands or engines, and benchmarks such as concolic-vs-default-benchmark need to control for all of this by hand.

## Evidence (re-verified at audit HEAD 56c86168)

- Random path hash: `shatter-core/src/explorer.rs:566-571` (`path_hash`: scope-aware + loop buckets, with the `legacy_path_hash` line/error/return fallback).
- Concolic path hash: `shatter-core/src/orchestrator.rs:864-871` (`hash_branch_path`: raw `(branch_id, taken)` sequence, `DefaultHasher`). Unique-path cap check: `orchestrator.rs:1621-1627`.
- Random explorer shrink selection calls `crate::orchestrator::hash_branch_path` (`explorer.rs:1704`, `:1788`, `:1833`), not `path_hash`. This is also the explorer<->orchestrator cycle tracked in str-qwua7.29.
- Fixture used by the audit (`areas/core-engine.md:10-25`):

  ```go
  func Loopy(n int) int {
      s := 0
      for i := 0; i < n && i < 50; i++ { s += i }
      if s > 100 { return 1 }
      return 0
  }
  ```

  Result: 16 paths under `--concolic` (40 iterations) vs 6 under random (100 iterations), for 2 behaviours. The concolic report lists 16 rows.
- `max_executions`:
  - `shatter-cli/src/commands/explore.rs:5145` = `max_iterations` (1x);
  - `shatter-core/src/scan_orchestrator.rs:3144-3150` `concolic_scan_max_executions` = 5x, or 1x with custom generators;
  - `shatter-cli/src/commands/observe.rs:109` = 5x.
- Help: `shatter-cli/src/args.rs:505` and `:992` read "Maximum number of iterations per function [default: 100]", and `:1379` reads "(default: 50)".
- The three literals:
  - `explore.rs:5138-5169`
  - `scan_orchestrator.rs:3080-3102`
  - `observe.rs:107-130`

  How they differ:

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

## Acceptance criteria

- [ ] Path identity lives in one leaf module (for example `shatter-core/src/path_identity.rs`) used by the random explorer, the concolic orchestrator and both shrinkers. This also removes the explorer<->orchestrator `hash_branch_path` cycle (coordinate with str-qwua7.29).
- [ ] One constructor (for example `orchestrator::ExploreConfig::from_explorer(&explorer::ExploreConfig, Overrides)`) builds the orchestrator config for explore, scan and observe. The three struct literals are gone (verify with `grep -n "orchestrator::ExploreConfig {"` over non-test code), and unit tests assert that shared fields round-trip.
- [ ] `--max-iterations` has one documented meaning across explore, scan, observe and run, and the help text in `args.rs` says what it bounds (unique paths vs executions).
- [ ] Engine-parity test: Loopy and `01-arithmetic` give equal path counts under random and concolic. Proof at close: the test fails on current main (paste the failure) and passes after the change. It can be added as a row in the engine_parity suite (engine-parity-e2e) if that lands first.

## Suggested approach

Coordinate with str-qwua7.6.2 (shared ExploreConfig base; it does not name the scan or observe literals) and str-qwua7.20.1. Either do the work there and append this issue's scope as a note, or do it here as a child with those linked. Land the path-identity module first; the budget and config unification follows.

## Out of scope

- Splitting `explore_with_oracle` into phases (str-qwua7.6).
- Other random-vs-concolic drifts: setup, mocks, refine and capture have their own issues in shatter-engine-correctness.

## Metadata

- Priority: P2
- Type: task
- Labels: audit-2026-09-22, architecture, parity, explorer, orchestrator
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: str-qwua7.6.2, str-qwua7.20.1, str-qwua7.29, str-inct, engine-parity-e2e, concolic-vs-default-benchmark
- Source findings: core-07, core-14 (draft shatter-code/17)

---

<!-- file: 06-engine-parity-e2e.md -->

---
slug: engine-parity-e2e
kind: new
title: "Replace prose-only random-vs-concolic parity with gates: engine_parity E2E suite through the pipeline, `_`-param lint, close-reason call-site rule"
priority: P2
type: task
labels: [audit-2026-09-22, parity, e2e, testing, quality-gates]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Replace prose-only random-vs-concolic parity with gates: engine_parity E2E suite through the pipeline, `_`-param lint, close-reason call-site rule

## Problem

CLAUDE.md warns repeatedly about parallel code paths: the random explorer vs the concolic orchestrator, and the CLI wiring for `--concolic` vs the default. Enforcement, however, is only "grep for the parallel path". This audit found at least seven random-vs-concolic drifts that survived, each filed separately:

- `--setup` is ignored under concolic (concolic-setup-teardown);
- dynamic mock variation regressed (concolic-mock-variation-regression);
- the refine phase drops `prepare_id` and `execution_profile` (concolic-refine-execute-builder);
- shrinking runs after teardown;
- capture is hard-coded (str-qwua7.5);
- path identity differs (engine-path-identity-budget-config);
- float-probe paths are under-counted (float-probe-paths-uncounted).

Two issues were also closed on evidence from non-production paths: str-0s76.6, via a test that calls `orchestrator::explore` directly, and str-55ep, which fixed a dead shrinker copy.

The per-BranchType TS known-answer fixtures from the original draft are filed separately as ts-branchtype-known-answer-fixtures (bucket shatter-frontend-ts).

## Evidence (re-verified at audit HEAD 56c86168)

- `shatter-core/tests/e2e_concolic.rs:1553-1558`: `orchestrator_explore_with_setup_context` ("This is the parity test for the orchestrator path") injects setup context straight into `orchestrator::explore`. The production callers (`pipeline_orchestrator.rs:542`, `scan_orchestrator.rs:3109`, `observe.rs:186`, per core-03) pass `None`.
- `grep -c "pipeline_orchestrator\|run_pipeline"` gives `e2e_concolic.rs` 1, `e2e_concolic_go.rs` 0 and `e2e_concolic_rust.rs` 0. The Go and Rust E2E suites never exercise the pipeline wiring where drift happens.
- `_`-prefixed unused bindings hid the mock-variation regression:
  - `shatter-core/src/orchestrator.rs:2147`: `_mock_params: &[MockParam]`
  - `orchestrator.rs:2643`: `let _initial_mocks = ...`
- CLAUDE.md "Completion Checklist" (`CLAUDE.md:43`) accepts unit/API tests as proof of pipeline behaviour for anything except the named E2E commands. There is no requirement to name the production caller.

## Acceptance criteria

- [ ] New E2E suite `shatter-core/tests/engine_parity.rs`, wired into `task e2e`. It runs a table of fixtures × {random, concolic} through `pipeline_orchestrator`/`run_pipeline` or the CLI entry point, never `orchestrator::explore` or `explorer::explore_function` directly. It asserts:
  - path count;
  - reached lines;
  - that a `--setup` side effect is visible;
  - that a mock-dependent branch is reached;
  - that the capture flag is honoured.

  It covers at least one TS, one Go and one Rust fixture. Known-divergent cases are marked expected-fail with the tracking issue ID in the marker, so the suite runs green today and flips when each drift is fixed.
- [ ] A lint script, wired into `check-static`, flags `_`-prefixed parameters or `let` bindings in `shatter-core/src` and `shatter-cli/src` (non-test code) that lack a `TODO(str-...)` comment on the same or previous line. Existing hits are fixed or annotated.
- [ ] CLAUDE.md Completion Checklist gains a rule: a close reason for a pipeline feature or fix names the production call site exercised and the pipeline-level test that proves it. Test workarounds recorded only in CLAUDE.md prose (for example `shatter-ts/CLAUDE.md:283-287`, "the e2e reads `raw_results`" because switch emits no `branch_path`) must be filed as issues. Link bento close-reason-evidence (bento bucket) so the generic bento rule and this repo rule agree.
- [ ] Proof at close:
  - forced (uncached) `task e2e` output showing `engine_parity` executed, with its pass/expected-fail counts;
  - the lint script run on a planted `_unused` binding, showing it fails;
  - the CLAUDE.md diff.

## Suggested approach

Split into child tasks when claimed: (1) suite scaffold + TS rows, (2) Go/Rust rows, (3) lint, (4) CLAUDE.md rule. The path-count row depends on engine-path-identity-budget-config; start it as expected-fail.

## Out of scope

- Fixing the individual engine drifts (filed separately as product bugs).
- Per-BranchType TS fixtures (ts-branchtype-known-answer-fixtures).
- The module-reachability dead-code check (core-dead-code-removal).
- The module-graph cycle check (str-qwua7.29).

## Metadata

- Priority: P2
- Type: task
- Labels: audit-2026-09-22, parity, e2e, testing, quality-gates
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: ts-branchtype-known-answer-fixtures, engine-path-identity-budget-config, concolic-setup-teardown, concolic-mock-variation-regression, concolic-refine-execute-builder, float-probe-paths-uncounted, close-reason-evidence (bento); str-qwua7.29, str-qwua7.51, str-inct, str-qwua7.5, bento-m4en, bento-a0nz
- Source findings: core-22 (draft shatter-agent/23, split)

---

<!-- file: 07-core-dead-code-removal.md -->

---
slug: core-dead-code-removal
kind: new
title: "Remove ~3,400 lines of production-dead shatter-core code (recursive, array_mutation, reporter+clustering, sequential scan(), shrink_witness) and add a reachability gate"
priority: P2
type: chore
labels: [audit-2026-09-22, cleanup, shatter-core, tech-debt]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Remove ~3,400 lines of production-dead shatter-core code (recursive, array_mutation, reporter+clustering, sequential scan(), shrink_witness) and add a reachability gate

## Problem

Several shatter-core modules and functions have no non-test callers, yet they attract fixes, audit grades and planned proptests as if they were live, and they make parity reasoning harder. Examples:

- str-8q1b4 cites the dead sequential `scan()` as a fix site.
- str-qwua7.47 plans proptests for `array_mutation`.
- The prior audit graded clustering "Solid".

Only `export.rs` (1,743 lines) is tracked for deletion (str-qwua7.59). Because pub items in a lib crate never warn and cargo-machete covers only dependencies, nothing catches this.

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

The total is about 3,410 lines, excluding `export.rs`, which str-qwua7.59 already tracks. The earlier draft's "~5,100" included it.

Check used: `grep -rn "crate::<mod>::\|shatter_core::<mod>::\|use crate::<mod>\|super::<mod>"` over `shatter-core/src shatter-cli/src shatter-rust/src`, excluding the module's own file and `#[cfg(test)]` blocks.

## Acceptance criteria

- [ ] Each item above is either deleted, or wired into production behind a tracked issue whose ID appears in a comment at the item.
- [ ] `mutate_mock_values`: coordinate with concolic-mock-variation-regression. If that fix revives it as the production mock-variation path, it stays. Otherwise it is deleted.
- [ ] A module/function reachability check (script under `scripts/`, wired into `task check-static`) lists pub modules and pub fns in shatter-core with zero references outside their own file and tests. It supports an allowlist file with a reason per entry and fails on new unallowlisted entries.
- [ ] Note appended to str-qwua7.47: drop `array_mutation` from its proptest list, because the module is deleted here. This replaces the dropped duplicate finding core-17.
- [ ] Proof at close:
  - `cargo build --workspace` and forced `task check` output after the deletions;
  - the reachability script run directly (not via the cached task) on a branch with a planted unused pub fn, showing a failure;
  - the same script passing on the final branch.

## Suggested approach

One commit per module. Delete `reporter.rs` and `clustering.rs` together. Delete `scan()` together with its test and the doc reference at `:1223`. For the check, a small Python or `cargo +nightly rustdoc --output-format json`-based script is enough; do not add a heavy tool.

## Out of scope

- Deleting `export.rs` (str-qwua7.59).
- Unused-code tooling in the bento audit skill (bento-r85d).
- Refactors of live code in the same files.

## Metadata

- Priority: P2
- Type: chore
- Labels: audit-2026-09-22, cleanup, shatter-core, tech-debt
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: str-qwua7.59, str-qwua7.47, str-8q1b4, concolic-mock-variation-regression, engine-parity-e2e
- Source findings: core-08, core-17 (array_mutation part) (draft shatter-code/18)

---

<!-- file: 08-stable-hash-persisted-keys.md -->

---
slug: stable-hash-persisted-keys
kind: new
title: "std DefaultHasher keys persisted/reproducible values (core_sample --seed selection, Rust harness cache dirs); use a specified hash"
priority: P3
type: bug
labels: [audit-2026-09-22, rust, cache, reproducibility]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# std DefaultHasher keys persisted/reproducible values (core_sample --seed selection, Rust harness cache dirs); use a specified hash

## Problem

The std docs say the `DefaultHasher` algorithm is unspecified and may change between Rust releases. Shatter uses it for:

- the `scan --seed` sampling, which the help text promises is reproducible;
- on-disk Rust harness cache directories, whose compiled binaries are reused.

The repo has no `rust-toolchain.toml`. A toolchain bump can therefore silently change which functions `--seed`/`--batch next` selects, and invalidate or mis-key harness caches.

Impact is bounded: `DefaultHasher::new()` uses fixed SipHash keys, so results are stable within one toolchain. The practical effect is spurious rebuilds and non-reproducible sampling across toolchains. Wrong results would require a collision.

## Evidence (re-verified at audit HEAD 56c86168)

- `shatter-core/src/core_sample.rs:16` imports `DefaultHasher`. `default_seed` (`:388-389`) and `stable_hash` (`:549-550`) use it.
- `shatter-cli/src/args.rs:970-979`: the `--seed` help says "the same seed over unchanged source yields the same exploration".
- `shatter-rust/src/executor.rs` uses `DefaultHasher` in:
  - `native_replay_hash` (`:306-308`)
  - `mocks_hash` (`:736-739`)
  - `source_hash` (`:764-766`, which folds in `HARNESS_CACHE_VERSION = 2` from `:758`)
  - `stable_crate_harness_dir` (`:839-847`)
  - `stable_crate_bridge_dir` (`:3234-3236`)
  - `hash_native_replay_sources` (`:3734-3749`)
  - `runtime_source_hash` (`:4139-4175`)
- `executor.rs:6208-6218` `compute_prepare_id` already uses `sha2::Sha256`.
- `ls rust-toolchain*` finds no file.

## Acceptance criteria

- [ ] Every hash that is persisted to disk, used in a cache path, or promised reproducible uses a specified algorithm over an explicit byte encoding. Options: SHA-256 truncated to u64 (sha2 is already a dependency), FNV-1a, or fixed-key `siphasher`. In-memory-only uses (for example `orchestrator::hash_branch_path`) may keep `DefaultHasher`.
- [ ] `HARNESS_CACHE_VERSION` bumped once.
- [ ] Golden-value unit tests pin the hash output for fixed inputs (`default_seed("x")`, `stable_hash(...)`, `source_hash("...")`), so an algorithm change fails a test. Proof at close: test output.
- [ ] The rust-conventions skill (`.claude/skills/rust-conventions/`) gains a one-line rule: no `DefaultHasher` for persisted or promised-stable keys.

## Suggested approach

Mechanical replacement with a small `stable_hash_u64(&[u8]) -> u64` helper in each crate, or a shared one if a common crate fits the dependency direction (cli -> core; frontends -> protocol).

## Out of scope

- Adding a rust-toolchain pin (separate decision).
- Hashes that are never persisted.

## Metadata

- Priority: P3
- Type: bug
- Labels: audit-2026-09-22, rust, cache, reproducibility
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: none
- Related: str-0m0vn (--seed reproducibility), seed-for-explore-and-run
- Source findings: core-20, frontend-rust-13 (draft shatter-code/25)

---

<!-- file: 09-qwua7-6-function-length-ratchet.md -->

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

<!-- file: 10-qwua7-43-bench-dev-dep-cycle.md -->

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
