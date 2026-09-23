---
slug: gauntlet-scan-checker-consumes-json
kind: new
title: "Gauntlet scan-failure checker has matched nothing since 2026-05-13: consume scan JSON `failed[]`, regenerate test fixtures from the CLI, fix allowlist and CLAUDE.md"
priority: P1
type: bug
labels: [gauntlet, quality-gates, testing, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Gauntlet scan-failure checker has matched nothing since 2026-05-13; its tests pin the dead format

## Problem

The gauntlet's per-step output screen can no longer detect scan failures. Both regexes it relies on target output formats the CLI stopped producing. Its unit tests use hand-written strings in the old format, so they stay green, and nothing runs them anyway. A scan with 4 failed and 7 interrupted functions passes the checker. The Gauntlet-gate paragraph in CLAUDE.md describes a guarantee that does not hold. The checker and the CLI output share no contract, so changing a user-facing format triggered no check on downstream parsers.

Closed issues str-jeen.57 (gauntlet fails on scan FAIL/error rows) and str-jeen.59 (allowlist) delivered this check. It went dead five days after str-jeen.57 closed. A reopen-note on str-jeen.57 points here (`gauntlet-checker-reopen-note`). The open str-qwua7.10 covers bad sanity checks in the demo gates generally. This issue is the concrete fix for the scan-failure part.

## Evidence (re-verified 2026-09-23 against the audit snapshot, unchanged on main)

- `demo/gauntlet_check_output.py:30-35`: `SCAN_ERROR_SUMMARY_RE` requires `Scan complete: ... N error(s)`, and `FAIL_ROW_RE` requires `| FAIL |` markdown rows.
- Commit 00124c84 (2026-05-13, str-izhn) changed the scan summary to `Scan complete: {} completed, {} failed, {} unsupported, {} interrupted, {} skipped ({} worker(s))` (`shatter-core/src/scan_orchestrator.rs:6120`). Production code never emits `error(s)`.
- `shatter-core/src/report.rs:2302-2315` (str-4ad5) emits only PASS, WARN and LOW rows, and a test at `report.rs:4886` asserts `!md.contains("| FAIL |")`. As a result, every `outcome: FAIL` entry in `demo/gauntlet-scan-allowlist.yaml` (lines 29-64 and following) is dead as well.
- `demo/test_gauntlet_check_output.py:28-39` pins `Scan complete: **43 function(s)** tested, **0 skipped**, **2 error(s)**` and hand-written `| FAIL |` rows. No gate runs this module (see `wire-every-test-module`). It appears only as a path trigger at `scripts/affected-gates.py:177`.
- `expected_scan_errors` in the allowlist (`demo/gauntlet-scan-allowlist.yaml:108-112`) names `11-opaque-types.ts` and `12-external-deps.ts`, which are absent from the pinned examples checkout. CLAUDE.md's Gauntlet-gate paragraph (line 41) repeats those names and the dead "`FAIL` rows / `N error(s)`" description.
- There are four divergent copies of the step check: `demo/gauntlet.sh:353-374` (which calls the Python helper, with an inline regex fallback), `demo/walkthrough.sh:261`, `demo/walkthrough-docker.sh:144` and `demo/gauntlet-docker.sh:167`. The Docker gauntlet has no FAIL or summary check at all.
- Repro: `python3 demo/gauntlet_check_output.py --allowlist demo/gauntlet-scan-allowlist.yaml --output audits/2026-09-22/artifact-samples/scan-mix.stdout --step x` exits 0 with no output. That stdout reads `**1 completed**, **4 failed**, **0 unsupported**, **7 interrupted**`, and the matching `scan-mix.json` has `codebase.failed_functions == 4` and a non-empty `codebase.failed` array.

## Acceptance criteria

- [ ] The checker detects failed and interrupted scan functions from scan's JSON output (`--format json`: `codebase.failed_functions`, `codebase.failed[]`), or from a single documented machine-readable summary line. It no longer parses markdown prose. The gauntlet's scan steps emit that JSON, or write it to a file the checker reads.
- [ ] The checker's tests get fixtures from the real CLI: either by running it on a small example with a known-failing function, or from a checked-in fixture produced by the CLI with the regeneration command documented next to it. A test fails if the checker reports 0 failures on that fixture. The tests pinning `N error(s)` and `| FAIL |` are removed. `demo/test_gauntlet_check_output.py` is run by the `gauntlet` gate or by `meta`.
- [ ] Proof: the new test fails against the current checker, with the command and output recorded in the close reason, and passes after the rewrite. Running the checker on `audits/2026-09-22/artifact-samples/scan-mix.*` reports the 4 failed functions.
- [ ] The allowlist names only functions and files that exist in the pinned examples checkout, each with a tracker issue id. Stale entries (`11-opaque-types.ts`, `12-external-deps.ts`, dead `outcome: FAIL` rows) are removed or rewritten against the JSON fields.
- [ ] The CLAUDE.md Gauntlet-gate paragraph describes the new contract (JSON-based, allowlist semantics) and drops the removed fixture names.
- [ ] All demo scripts (`gauntlet.sh`, `gauntlet-docker.sh`, `walkthrough.sh`, `walkthrough-docker.sh`) use the one shared checker for scan steps.
- [ ] `task gauntlet` on current main either passes with an allowlist that matches reality, or fails listing the real failures. Record the evidence (run log path or excerpt) in the close reason.

## Suggested approach

Have each gauntlet scan step also write `--format json` output, for example with `-o <step>.json`. The checker loads it, diffs `codebase.failed[]` (function + file + reason) against the allowlist, and flags anything new. Keep `PROCESS_ERROR_RE` for process-level markers. Generate the test fixture with a documented `shatter scan --format json` command over a tiny example containing one deliberately failing function.

## Out of scope

- Fixing the underlying scan failures themselves.
- A CLI golden-output contract suite (`golden-and-consumer-suite`, shatter-reports-and-specs bucket, which is blocked by this issue).
- General demo-gate sanity checks beyond scan failures (str-qwua7.10).

## Metadata

- Priority: P1. This overrides the P2 in the report tables, per report §15.1 and §14 item 25. Type: bug. Size: M.
- Labels: gauntlet, quality-gates, testing, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none.
- Related: str-jeen.57 (closed), str-jeen.59 (closed), str-qwua7.10, str-izhn, str-4ad5.
- Source findings: artifacts-07. Draft shatter-agent/09.
