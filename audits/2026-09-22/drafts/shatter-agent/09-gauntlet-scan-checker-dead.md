# Gauntlet scan-failure checker has matched nothing since 2026-05-13; tests pin the dead format

- Priority: P1
- Type: bug
- Labels: gauntlet,quality-gates,testing,agents
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (str-jeen.57 closed-but-unfixed; related str-qwua7.10)
- Source findings: artifacts-07
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
The gauntlet's per-step output screen cannot detect scan failures any more.
Both regexes it relies on target output formats the CLI stopped producing,
and its unit tests use hand-written strings in the old format, so they stay
green. A scan with 4 failed and 7 interrupted functions passes the checker.
CLAUDE.md's Gauntlet-gate paragraph describes a guarantee that does not hold.

## Current Code Facts
- `demo/gauntlet_check_output.py:28-33`: `SCAN_ERROR_SUMMARY_RE` matches
  "N error(s)"; `FAIL_ROW_RE` matches `| FAIL |` rows.
- Commit 00124c84 (2026-05-13, str-izhn) changed the summary to
  `Scan complete: **N completed**, **M failed**, ...`
  (`shatter-core/src/scan_orchestrator.rs:6120`). Production code never
  emits "error(s)".
- `shatter-core/src/report.rs:2302-2315` (str-4ad5) emits only PASS/WARN/LOW;
  a test at `report.rs:~4886` asserts no `| FAIL |` row. So every
  `outcome: FAIL` entry in `demo/gauntlet-scan-allowlist.yaml` is dead too.
- `demo/test_gauntlet_check_output.py:31` pins
  `Scan complete: **1 function(s)** tested, **0 skipped**, **0 error(s)**`.
  This test module is not run by any gate (see draft 19).
- Allowlist `expected_scan_errors` names `11-opaque-types.ts` and
  `12-external-deps.ts` (lines ~79-84), absent from the examples checkout.
- Four copies of step-check logic: `demo/walkthrough.sh:261`,
  `demo/walkthrough-docker.sh:144`, `demo/gauntlet-docker.sh:167`, plus the
  Python checker; the Docker gauntlet has no FAIL/summary check.
- Repro: run the checker over `audits/2026-09-22/artifact-samples/scan-mix.*` stdout
  (4 failed) -> exit 0, no output.

## Acceptance Criteria
- The checker detects failed/interrupted scan functions from scan's JSON
  (`--format json`: `codebase.failed_functions` / `failed[]`) or from one
  documented machine-readable summary line, not from markdown prose.
- Checker tests generate fixtures by running the real CLI on a small example
  containing a known-failing function (or load a checked-in fixture produced
  by the CLI with a regeneration command documented next to it); the test
  fails if the checker reports 0 failures on that fixture.
- Allowlist entries reference functions/files that exist, each with a tracker
  issue ID; stale entries removed. CLAUDE.md Gauntlet-gate paragraph updated.
- All demo scripts use the one shared checker.
- `task gauntlet` on current main either passes with an allowlist that matches
  reality or fails listing the real failures (evidence in close reason).

## Out of Scope
Fixing the underlying scan failures. Golden-output suite (draft 24).
