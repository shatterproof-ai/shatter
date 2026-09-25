---
slug: gauntlet-scan-checker-consumes-json
kind: new
title: "Gauntlet scan-failure checker has matched nothing since 2026-05-13: consume scan JSON (`codebase.failed[]` and interrupted `skipped_functions[]`), regenerate test fixtures from the CLI, fix allowlist and CLAUDE.md"
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

The gauntlet's per-step output screen can no longer detect scan failures or interruptions. Both regexes it relies on target output formats the CLI stopped producing. Its unit tests use hand-written strings in the old format, so they stay green, and nothing runs them anyway. A scan with 4 failed and 7 interrupted functions passes the checker. The Gauntlet-gate paragraph in CLAUDE.md describes a guarantee that does not hold. The checker and the CLI output share no contract, so changing a user-facing format triggered no check on downstream parsers.

Closed issues str-jeen.57 (gauntlet fails on scan FAIL/error rows) and str-jeen.59 (allowlist) delivered this check. It went dead five days after str-jeen.57 closed. A reopen-note on str-jeen.57 points here (`gauntlet-checker-reopen-note`). The open str-qwua7.10 covers bad sanity checks in the demo gates generally. This issue is the concrete fix for the scan-failure part.

## Evidence (re-verified 2026-09-23 on main `70465921`)

- `demo/gauntlet_check_output.py:30-35`: `SCAN_ERROR_SUMMARY_RE` requires `Scan complete: ... N error(s)`, and `FAIL_ROW_RE` requires `| FAIL |` markdown rows.
- Commit 00124c84 (2026-05-13, str-izhn) changed the scan summary to `Scan complete: {} completed, {} failed, {} unsupported, {} interrupted, {} skipped ({} worker(s))` (the format string in `shatter-core/src/scan_orchestrator.rs`; line 6301 on main, 6120 on the audit snapshot). Production code never emits `error(s)`.
- `shatter-core/src/report.rs` (str-4ad5) emits only PASS, WARN and LOW rows in the markdown scan table, and a test in the same file asserts `!md.contains("| FAIL |")`. As a result, every `outcome: FAIL` entry in `demo/gauntlet-scan-allowlist.yaml` is dead as well.
- In scan's JSON output, failures and interruptions live in different fields. Failed functions are in `codebase.failed[]` (count in `codebase.failed_functions`). Interrupted functions are **not** failures: they are entries in `codebase.skipped_functions[]` with `category == "interrupted"` (`report.rs`, str-smcx; the `category: "interrupted".into()` constructor). A checker that reads only `failed[]` misses every interruption.
- `demo/test_gauntlet_check_output.py:28-39` pins `Scan complete: **43 function(s)** tested, **0 skipped**, **2 error(s)**` and hand-written `| FAIL |` rows. No gate runs this module (see `wire-every-test-module`). It appears only as a path trigger at `scripts/affected-gates.py:177`.
- `expected_scan_errors` in the allowlist (`demo/gauntlet-scan-allowlist.yaml:108-112`) names `11-opaque-types.ts` and `12-external-deps.ts`, which are absent from the pinned examples checkout. CLAUDE.md's Gauntlet-gate paragraph repeats those names and the dead "`FAIL` rows / `N error(s)`" description.
- There are four divergent copies of the step check: `demo/gauntlet.sh:353-374` (which calls the Python helper, with an inline regex fallback), `demo/walkthrough.sh:261`, `demo/walkthrough-docker.sh:144` and `demo/gauntlet-docker.sh:167`. The Docker gauntlet has no FAIL or summary check at all.
- Some gauntlet scan steps are expected to produce non-standard results: `scan --timeout-total 120 --timeout-per-fn 30` (`demo/gauntlet.sh:641`) may legitimately interrupt functions, and `scan --core-sample 3 --dry-run` (`:585`) executes nothing.
- Audit repro sample (committed on the audit branch only, not on main): `git show 56c86168:audits/2026-09-22/artifact-samples/scan-mix.stdout` and `...scan-mix.json`. `python3 demo/gauntlet_check_output.py --allowlist demo/gauntlet-scan-allowlist.yaml --output <that stdout> --step x` exits 0 with no output. The stdout reads `**1 completed**, **4 failed**, **0 unsupported**, **7 interrupted**`; the JSON has `codebase.failed_functions == 4`, 4 `failed[]` entries, and 7 `skipped_functions[]` entries with category `interrupted`.
- Cheaper partial mechanism: `shatter scan --fail-on-failures[=PERCENT]` (`shatter-cli/src/args.rs`, str-izhn) exits non-zero on failed attempts. It cannot express a per-function allowlist and does not cover interruptions, so it complements rather than replaces the JSON check.

## Acceptance criteria

- [ ] The checker reads scan's JSON output (`--format json`). It flags (a) every `codebase.failed[]` entry and (b) every `codebase.skipped_functions[]` entry with `category == "interrupted"`, each unless allowlisted by function + file (+ reason for failures). It no longer parses markdown prose. The gauntlet's scan steps write that JSON to a file the checker reads.
- [ ] Intentional cases are explicit, not implicit: the `--timeout-total` step declares in the allowlist (or a per-step config) that interruptions are expected there, and the `--dry-run` step is marked as producing no scan JSON and skipped by the checker with a logged reason. A step that produces no JSON when one was expected is an error.
- [ ] The checker's tests use fixtures produced by the real CLI, with the regeneration command documented next to them: one fixture with at least one failed function and one with only interrupted functions (for example a tiny example plus `--timeout-per-fn 1` on a function that loops). Tests assert the checker reports the failure in the first and the interruption in the second, and reports 0 for an allowlisted copy of each. The tests pinning `N error(s)` and `| FAIL |` are removed.
- [ ] `demo/test_gauntlet_check_output.py` is wired into `meta` (reachable from `check`, as `wire-every-test-module` requires). If `wire-every-test-module` added a temporary allowlist entry for it, that entry is removed here.
- [ ] Proof: the new fixture tests fail against the current checker, with the command and output recorded in the close reason, and pass after the rewrite.
- [ ] The allowlist names only functions and files that exist in the pinned examples checkout, each with a tracker issue id. Stale entries (`11-opaque-types.ts`, `12-external-deps.ts`, dead `outcome: FAIL` rows) are removed or rewritten against the JSON fields.
- [ ] The CLAUDE.md Gauntlet-gate paragraph describes the new contract (JSON-based, failed + interrupted, allowlist semantics) and drops the removed fixture names.
- [ ] All demo scripts (`gauntlet.sh`, `gauntlet-docker.sh`, `walkthrough.sh`, `walkthrough-docker.sh`) use the one shared checker for scan steps.
- [ ] `task gauntlet` on current main either passes with an allowlist that matches reality, or fails listing the real failures and interruptions. Record the run log path or excerpt in the close reason.

## Suggested approach

Have each gauntlet scan step also write `--format json` output, for example with `-o <step>.json`. The checker loads it, collects `failed[]` and interrupted `skipped_functions[]`, diffs them against the allowlist, and flags anything new. Keep `PROCESS_ERROR_RE` for process-level markers. Optionally also pass `--fail-on-failures` so a non-zero exit backs up the JSON check on steps with no allowlisted failures.

## Out of scope

- Fixing the underlying scan failures themselves.
- A CLI golden-output contract suite (`golden-and-consumer-suite`, shatter-reports-and-specs bucket, which is blocked by this issue).
- General demo-gate sanity checks beyond scan failures (str-qwua7.10).

## Metadata

- Priority: P1. This overrides the P2 in the report tables, per report §15.1 and §14 item 25. Type: bug. Size: M.
- Labels: gauntlet, quality-gates, testing, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none.
- Related: str-jeen.57 (closed), str-jeen.59 (closed), str-qwua7.10, str-izhn, str-4ad5, str-smcx, `wire-every-test-module`.
- Source findings: artifacts-07. Draft shatter-agent/09.
