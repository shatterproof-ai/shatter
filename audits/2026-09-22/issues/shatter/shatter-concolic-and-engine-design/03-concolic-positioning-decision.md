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
