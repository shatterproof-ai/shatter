---
slug: golden-and-consumer-suite
kind: new
title: "Producer/consumer contract suite for CLI artifacts: CLI-driven golden outputs, cross-format counts, consumer round-trips"
priority: P2
type: task
labels: [testing, artifacts, report, quality-gates, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [gauntlet-scan-checker-consumes-json]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Producer/consumer contract suite for CLI artifacts: CLI-driven golden outputs, cross-format counts, consumer round-trips

## Problem

Most output bugs found by the 2026-09-22 audit are drift between a producer and its consumer:

- the gauntlet checker's regex against the scan summary (gauntlet-scan-checker-consumes-json);
- `compare` rejecting the `--spec-out` bundle (spec-json-shapes-compare);
- SPEC §5 samples vs real output (spec-s5-contract-table-and-samples);
- HTML "Paths Found" showing branches while markdown shows paths (scan-report-headline-and-paths);
- `explore -o x.json` writing an empty bundle (explore-o-json-empty-bundle);
- plugin skills documenting flags the CLI does not have.

No test tier owns output correctness. Output tests do exist, but they do not catch these bugs: the insta snapshot tests in `shatter-core/tests/` render synthetic in-memory reports, so they pin whatever the renderer does today, including current bugs. For example, the HTML snapshot pins "Paths Found" = sum of `branches_covered`. None of them runs the CLI on a known-answer example, compares counts across formats, or feeds one command's output to its consumer.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- Snapshot tests: `shatter-core/tests/html_snapshots.rs`, `outcome_md_snapshots.rs`, `run_markdown_ordering_snapshots.rs`, `source_set_summary_snapshots.rs`, with files in `shatter-core/tests/snapshots/`. They build reports in memory.
- They pin a known bug: `shatter-core/src/html_templates.rs:382` computes `total_paths` as the sum of `branches_covered`, and the HTML snapshot records that output.
- `task golden-test` (Taskfile.yml, "Run cross-frontend parity golden tests") covers protocol goldens only.
- `shatter-cli/tests/` has contract tests (`json_stdout_contract.rs`, `exit_code_conventions.rs`, and others) but no terminal-output or report-output goldens for explore/scan/spec-out/spec-diff/compare.
- The snapshot helpers also silently create a missing snapshot and pass (`shatter-core/tests/outcome_md_snapshots.rs:45-51`); that is owned by snapshot-test-helpers, not by this issue.

## Acceptance criteria

- [ ] A new task (for example `task golden-cli`) runs `explore`, `scan`, `explore --spec-out`, `spec-diff` and `compare` on known-answer examples (TS `classifyNumber`, Go `ClassifyNumber`, Rust `classify_number`). It compares normalized output (paths made relative, timings and absolute temp dirs stripped) against checked-in goldens, and has an explicit update command.
- [ ] Cross-format assertion: for the same run, function counts (discovered/attempted/completed/failed), path counts and class counts are equal across markdown, JSON and HTML.
- [ ] Consumer round-trips: `explore --spec-out` -> `compare`; `explore --spec-out` -> `spec-diff`; scan JSON -> the gauntlet checker (as rewritten by gauntlet-scan-checker-consumes-json).
- [ ] Goldens that encode a known open bug are marked with the tracker id of that bug, so fixing the bug updates the golden deliberately.
- [ ] `scripts/affected-gates.py` selects the new task for changes to `shatter-core/src/report.rs`, `spec*.rs`, `html_templates.rs`, `shatter-cli/src/render.rs`, `shatter-core/templates/` and `demo/`. A selector test proves it.
- [ ] The new task is part of `task check`. Proof at close: a forced run (not a cached "up to date") showing the task executed and passed, plus one deliberately broken renderer change that makes it fail.

## Suggested approach

Build on the existing insta setup instead of adding a new framework: add CLI-driven snapshot tests that invoke the built binary on the examples repo, run a normalizer over the output, and snapshot the result. Add the cross-format and round-trip checks as ordinary tests in the same task.

## Out of scope

- Fixing the individual output bugs (filed separately in this bucket and others).
- Snapshot helper hygiene, such as failing on a missing snapshot under CI (snapshot-test-helpers).

## Dependencies

Blocked by gauntlet-scan-checker-consumes-json (the scan JSON -> gauntlet checker round-trip needs the rewritten checker).

## Related

str-qwua7.10, str-qwua7.53, str-qwua7.9, str-wurp, str-7jgm.3. Source finding: artifacts-18 (partially confirmed; the verifier noted that insta snapshot tests exist but are synthetic and pin current bugs, so this extends them rather than creating a tier from scratch).
