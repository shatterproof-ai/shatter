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
