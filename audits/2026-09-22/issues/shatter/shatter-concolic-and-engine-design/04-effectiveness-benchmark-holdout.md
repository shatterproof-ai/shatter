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
- **Observable failure.** A fault counts as **detected** when a Shatter run on the mutated source produces at least one input whose observed outcome (return value, thrown error, or declared side effect) differs from the outcome of the **unmutated** source on that same input, as reported by spec-diff (the regression tool per D2). Only the run's own generated inputs count. Inputs copied in from the fault manifest do not.
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
