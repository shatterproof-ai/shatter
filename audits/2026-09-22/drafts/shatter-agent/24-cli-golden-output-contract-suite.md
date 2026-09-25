# Producer/consumer contract suite for CLI artifacts: CLI-driven golden outputs, cross-format counts, consumer round-trips

- Priority: P2
- Type: task
- Labels: agents,testing,artifacts,report,quality-gates
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (related str-qwua7.10, str-qwua7.53, str-wurp)
- Source findings: artifacts-18
- Parent: 01 (epic)
- Blocked by: 09
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
Most output bugs found by this audit are drift between a producer and a
consumer: gauntlet checker vs scan summary (draft 09), `compare` rejecting the
`--spec-out` bundle, SPEC §5 samples vs real output, HTML "Paths Found" showing
branches vs markdown paths, `explore -o x.json` writing an empty bundle, plugin
skills vs CLI flags. No tier owns output correctness.

## Current Code Facts
- insta snapshot tests exist but render synthetic in-memory reports:
  `shatter-core/tests/html_snapshots.rs`, `outcome_md_snapshots.rs`,
  `run_markdown_ordering_snapshots.rs`, `source_set_summary_snapshots.rs`
  (snapshots in `shatter-core/tests/snapshots/`). They pin current output,
  including `html_templates.rs:382` total_paths = sum(branches_covered).
- `task golden-test` covers protocol goldens only.
- Four snapshot helpers self-create missing snapshots and pass
  (e.g. `outcome_md_snapshots.rs:45-51`).
- No CLI-level terminal-output snapshots in `shatter-cli/tests/`.

## Acceptance Criteria
- New task (e.g. `task golden-cli`) runs explore/scan/spec-out/spec-diff/compare
  on known-answer examples (TS classifyNumber, Go ClassifyNumber, Rust
  classify_number) and compares normalized output (paths relativised, timings
  and absolute temp dirs stripped) to checked-in goldens, with an explicit
  update command.
- Cross-format assertion: path/class counts equal across markdown, JSON and HTML
  for the same run.
- Consumer round-trips: `explore --spec-out` → `compare` and → `spec-diff`;
  scan JSON → gauntlet checker.
- `affected-gates.py` selects it for changes to `report.rs`, `spec*.rs`,
  `render.rs`, `templates/`, `demo/`.
- Snapshot helpers fail on a missing snapshot when `CI=1`.

## Out of Scope
Fixing the individual output bugs (filed separately).
