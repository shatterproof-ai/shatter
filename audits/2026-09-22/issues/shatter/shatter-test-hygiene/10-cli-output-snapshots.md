---
slug: cli-output-snapshots
kind: new
title: "CLI explore/scan output snapshots"
priority: P3
type: task
labels: [testing, cli, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [snapshot-test-helpers, pin-examples-repo]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# CLI explore/scan output snapshots

## Problem

No test pins the human-facing terminal output of `shatter explore`, `shatter scan` or the top-level `shatter --help`. Regressions in that output are caught only by manual walkthrough review. Split out of `snapshot-test-helpers` so the helper correctness fix is not held up by this new coverage.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- Full-output snapshots already exist for two subcommand help screens: `shatter-cli/tests/hide_exec_flags_help.rs:47-68` compares `spec-diff --help` and `doctor --help` exactly against `shatter-cli/tests/fixtures/help/*.txt` (regeneration command in the file header, lines 5-7). No equivalent exists for top-level `shatter --help`, `explore` output or `scan` output.
- `shatter-cli/tests/json_stdout_contract.rs` checks JSON structure only.
- `standalone/ts/01-arithmetic.*` and `standalone/go/01-arithmetic.*` exist in the external examples repo, which is not pinned until `pin-examples-repo` lands. Snapshots over unpinned inputs would be flaky by construction.
- Audit finding tests-ci-08 (the CLI-snapshot part, verified).

## Acceptance criteria

- [ ] Top-level `shatter --help` gets a full-output fixture using the existing `tests/fixtures/help/` convention (or the shared helper chosen in `snapshot-test-helpers`, if that moved the convention).
- [ ] New snapshots cover the terminal (non-JSON) output of `shatter explore` and `shatter scan` on the pinned `01-arithmetic` TS and Go examples.
- [ ] Redaction is explicit and minimal: absolute paths are made relative, and durations, timestamps and PIDs are replaced by fixed tokens. Each redaction rule is listed in the test file with the reason. No other normalization (in particular no whitespace collapsing).
- [ ] A missing fixture fails the test (same rule as `snapshot-test-helpers`). Proof at close: delete one new fixture, paste the failing run, restore, paste the passing run.
- [ ] Determinism proof: run the new tests 3 times in a row, and once with a different `TMPDIR` and working directory, and paste that all runs pass.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Reuse the `run_help`/`fixture` shape from `hide_exec_flags_help.rs` for `--help`. For explore/scan, use the shared helper plus a small redaction function. Keep the explored function set small so the snapshots stay readable.

## Out of scope

- Snapshotting every subcommand's help (only top-level is new here).
- JSON output contracts (already covered by `json_stdout_contract.rs`).

## Priority / type / labels

P3 · task · testing, cli, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: `snapshot-test-helpers` (shared helper convention), `pin-examples-repo` (stable example inputs).
