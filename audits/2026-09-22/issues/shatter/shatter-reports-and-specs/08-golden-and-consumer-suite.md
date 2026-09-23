---
slug: golden-and-consumer-suite
kind: new
title: "Producer/consumer contract suite for CLI artifacts: CLI-driven golden outputs, cross-format counts, consumer round-trips"
priority: P2
type: task
labels: [testing, artifacts, report, quality-gates, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [gauntlet-scan-checker-consumes-json, spec-json-shapes-compare, scan-report-headline-and-paths, pin-examples-repo]
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

No test tier owns output correctness. Output tests do exist, but they do not catch these bugs: the snapshot tests in `shatter-core/tests/` (hand-written file-comparison helpers, not the insta crate) render synthetic in-memory reports, so they pin whatever the renderer does today, including current bugs. For example, the HTML snapshot pins "Paths Found" = sum of `branches_covered`. None of them runs the CLI on a known-answer example, compares counts across formats, or feeds one command's output to its consumer.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- Snapshot tests: `shatter-core/tests/html_snapshots.rs`, `outcome_md_snapshots.rs`, `run_markdown_ordering_snapshots.rs`, `source_set_summary_snapshots.rs`, with files in `shatter-core/tests/snapshots/`. They build reports in memory. Each file defines its own `fn assert_snapshot(path, actual)` (for example `html_snapshots.rs:63`); no crate in the workspace depends on `insta`.
- `shatter-cli/tests/scan_seed_reproducibility.rs:1-30` records that `--seed` does not make a scan fully deterministic: parallel scheduling and wall-clock timeouts still leak nondeterminism, so that test asserts per-function coverage rather than byte-identical reports. `explore` has no `--seed` flag; `scan` does.
- They pin a known bug: `shatter-core/src/html_templates.rs:382` computes `total_paths` as the sum of `branches_covered`, and the HTML snapshot records that output.
- `task golden-test` (Taskfile.yml, "Run cross-frontend parity golden tests") covers protocol goldens only.
- `shatter-cli/tests/` has contract tests (`json_stdout_contract.rs`, `exit_code_conventions.rs`, and others) but no terminal-output or report-output goldens for explore/scan/spec-out/spec-diff/compare.
- The snapshot helpers also silently create a missing snapshot and pass (`shatter-core/tests/outcome_md_snapshots.rs:45-51`); that is owned by snapshot-test-helpers, not by this issue.

## Reproducibility contract

Exploration results (which inputs are found, which classes form) are not byte-stable across runs, so the suite must not golden-test them byte for byte. The contract:

- **Fixtures.** The examples corpus is read at the SHA pinned by pin-examples-repo (this issue is blocked by it). The fixture functions are TS `classifyNumber`, Go `ClassifyNumber` and Rust `classify_number`, whose branches are all reachable from mined literals within a small budget.
- **Budgets.** Every run uses an iteration budget (`--max-iterations`), never only a wall-clock limit, plus `--parallelism 1`, `--no-seeds`, `--no-cache` (or a fresh `--cache-dir` per test), a fresh temp project directory, and `--seed` where the command has one (`scan`).
- **What is compared byte for byte:** only normalized structure that does not depend on search luck: section headings and order, table columns, labels, and the set of class postconditions for the fixture functions (these are fully reachable, so the set is stable). Normalization strips timings, iteration counts, absolute and temp paths, and example inputs.
- **What is compared as facts, not bytes:** counts (functions discovered/attempted/completed/failed, path counts, class counts) must be equal across the formats of one run, and must equal the known answer for the fixture functions (4 classes for `classifyNumber`).
- **Stability proof.** Before the suite is added to `task check`, it is run 10 times in a row on one machine with zero differences; the close comment records the command and the result.
- **Baseline policy.** Goldens change only through the explicit update command, in the same commit as the code change that causes them, and the commit message says why.

## Acceptance criteria

- [ ] A new task (for example `task golden-cli`) runs `explore`, `scan`, `explore --spec-out`, `spec-diff` and `compare` on the fixture functions under the reproducibility contract above, compares normalized output against checked-in goldens, and has an explicit update command.
- [ ] Cross-format assertion: for the same scan run, function counts (discovered/attempted/completed/failed), path counts and class counts are equal across markdown, JSON and HTML. (Passes only after scan-report-headline-and-paths, which is why this issue is blocked by it.)
- [ ] Consumer round-trips: `explore --spec-out` → `compare` (TS vs Go `classifyNumber`, asserts 4 of 4; needs spec-json-shapes-compare); `explore --spec-out` → `spec-diff` on two runs of the same source (asserts exit 0, no ADDED/REMOVED/CHANGED); scan JSON → the gauntlet checker (as rewritten by gauntlet-scan-checker-consumes-json).
- [ ] Goldens that still encode a known open bug when this lands are listed in a checked-in file with the tracker id of that bug, so fixing the bug updates the golden deliberately. The three blocking bugs above may not appear in that list.
- [ ] `scripts/affected-gates.py` selects the new task for changes to `shatter-core/src/report.rs`, `spec*.rs`, `html_templates.rs`, `shatter-cli/src/render.rs`, `shatter-core/templates/` and `demo/`. A selector test proves it.
- [ ] The new task is part of `task check`. Proof at close: a forced run (not a cached "up to date") showing the task executed and passed; the 10-run stability result; and one deliberately broken renderer change (for example reverting the HTML Paths fix) that makes the task fail.

## Suggested approach

Reuse the pattern of the existing snapshot helpers (compare whitespace-normalized output against a file under a `snapshots/` directory; today they regenerate when the file is deleted), but give the new suite an explicit update mode (for example an environment variable read by the task's update command), invoke the built CLI binary on the fixture functions instead of rendering in-memory reports, and run a normalizer over the output first. Unlike the existing helpers, a missing golden must fail, not be written silently. Add the cross-format and round-trip checks as ordinary tests in the same task.

## Out of scope

- Fixing the individual output bugs (filed separately in this bucket and others).
- Snapshot helper hygiene, such as failing on a missing snapshot under CI (snapshot-test-helpers).

## Dependencies

- gauntlet-scan-checker-consumes-json: the scan JSON → gauntlet checker round-trip needs the rewritten checker.
- spec-json-shapes-compare: the `--spec-out` → `compare` round-trip cannot pass until `compare` reads bundles.
- scan-report-headline-and-paths: the HTML/markdown path-count equality cannot pass until the HTML shows paths.
- pin-examples-repo: the fixture functions must come from a pinned examples revision.

## Related

str-qwua7.10, str-qwua7.53, str-qwua7.9, str-wurp, str-7jgm.3. Source finding: artifacts-18 (partially confirmed; the verifier noted that snapshot tests exist but are synthetic and pin current bugs, so this extends the pattern rather than creating a tier from scratch).
