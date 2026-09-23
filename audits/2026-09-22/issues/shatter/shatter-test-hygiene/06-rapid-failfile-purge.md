---
slug: rapid-failfile-purge
kind: new
title: "Purge the still-tracked March rapid failfile and ignore rapid failfiles repo-wide (str-qwua7.4 left incomplete)"
priority: P3
type: chore
labels: [go, testing, cleanup, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Purge the still-tracked March rapid failfile and ignore rapid failfiles repo-wide (str-qwua7.4 left incomplete)

## Problem

str-qwua7.4 (closed 2026-09-22) required that `testdata/rapid/**/*.fail` be removed from git and added to `shatter-go/.gitignore`. The fix only covered the `planner/` package named in that issue. A rapid failfile from March is still tracked under `instrument/`, and the ignore rule does not cover any other package. The next rapid failure in any package other than `planner/` will show up as an untracked file that is easy to commit by accident.

## Evidence

Checked against `origin/main` 70465921 (2026-09-23).

- `git ls-tree -r --name-only origin/main | grep '\.fail$'` shows:
  `shatter-go/instrument/testdata/rapid/TestPropertyExecTimeoutAlwaysPositive/TestPropertyExecTimeoutAlwaysPositive-20260306134644-2197485.fail`
  (added in f0a58576, 2026-03-06, "feat: add property-based testing across all components (str-ho1d)").
- `shatter-go/.gitignore` in full:
  ```
  # rapid property-test failure artifacts (regenerated locally on failure;
  # not meant to be committed — see str-qwua7.4)
  planner/testdata/rapid/**/*.fail
  ```
- str-qwua7.4's close reason lists the gates that ran but does not re-check the purge acceptance bullet.
- Audit findings tests-ci-15 and prior-23 (both verified, P3).

## Acceptance criteria

- [ ] The `instrument/...2197485.fail` file is removed with `git rm`.
- [ ] `shatter-go/.gitignore` uses `**/testdata/rapid/**/*.fail` in place of the `planner/`-only rule.
- [ ] Proof at close: paste the empty output of `git ls-files '*.fail'`, and the output of `git check-ignore -v shatter-go/protocol/testdata/rapid/X/X.fail` showing the new rule matches a non-planner path.
- [ ] Before deleting the failfile, confirm whether `TestPropertyExecTimeoutAlwaysPositive` (`shatter-go/instrument/property_test.go:86`) still passes. If the failfile encodes a real counterexample that still fails, file it separately instead of discarding the evidence.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Two-line change plus `git rm`. Run `go test ./instrument/ -run TestPropertyExecTimeoutAlwaysPositive -count=3` first.

## Out of scope

- Other rapid or property-test cleanup.
- The TestPlanParam fix that str-qwua7.4 also covered.

## Priority / type / labels

P3 · chore · go, testing, cleanup, audit · Size S

## Parent epic

Epic: Audit 2026-09-22 findings (shatter)

## Dependencies

- Blocked by: none.
- Related: str-qwua7.4 (closed; receives `rapid-failfile-reopen-note`).
