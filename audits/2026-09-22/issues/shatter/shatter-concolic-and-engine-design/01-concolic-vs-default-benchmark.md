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
