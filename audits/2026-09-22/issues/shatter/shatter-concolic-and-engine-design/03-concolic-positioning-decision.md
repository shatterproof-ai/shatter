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
