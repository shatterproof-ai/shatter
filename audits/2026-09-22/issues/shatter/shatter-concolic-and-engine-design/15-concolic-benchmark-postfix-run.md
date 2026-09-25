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
