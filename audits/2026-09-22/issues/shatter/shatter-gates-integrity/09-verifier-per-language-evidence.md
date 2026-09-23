---
slug: verifier-per-language-evidence
kind: new
title: "Landing evidence must cover each changed language: land verifier runs no TS/Go/rust-fe tests, hides output, reports no executed flag; /pre-completion has no coverage or PBT row"
priority: P2
type: task
labels: [landing, quality-gates, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Landing evidence must cover each changed language: the verifier runs no TS/Go/rust-fe tests and hides output

## Problem

The land-work verifier gates landings with checks that do not cover the changed code, and it throws their output away. `/pre-completion` has no row checking that the gate evidence covers each changed language or crate. It also has no property-test row, even though CLAUDE.md Completion Checklist item 2 requires one, and it never says that `Task "X" is up to date` means the leaf did not run. Together with Task checksum caching, a landing can go green without the relevant tests running. There are three definitions of "the gate" (verifier, CI, pre-completion), and nothing checks free-text close reasons against the diff.

Related: str-qwua7.55 (open) covers the verifier's false "same gates as CI" claim and a verifier.json timeout. str-35vtk.24 (open) makes the verifier run one exact `task check`. str-qwua7.2 (open) covers executed-vs-cached receipts. bento already ships per-check `executed` (bento-rdtn.6) and verifier log persistence (bento-rdtn.4), but shatter has not adopted them. This issue owns the per-language coverage rows, the output and `executed` reporting, and the adoption of rdtn.4 and rdtn.6.

## Evidence (re-verified 2026-09-23 against the audit snapshot, unchanged on main)

- `scripts/land_work_verifier.sh`, last changed in d01a22db on 2026-08-05:
  - Header (`:3`) claims "Runs the same gates ci.yml uses". CI runs `task check` plus the shatter-llm steps (`.github/workflows/ci.yml:88-103`).
  - It runs `task test-standard`, `task parity` and `task conformance` only (`:25-27`).
  - `run_check` (`:14-23`) runs each check as `"$@" >/dev/null 2>&1`, so bento-rdtn.4's persisted log is empty on failure.
  - Per check it emits only `{name, status}`: no `executed`, no `wall_seconds`. bento's all-cached guard therefore cannot engage.
- `test-standard` (`Taskfile.yml:103-111`) = `frontends-built` + `workspace-clippy` + `workspace-test` (`cargo test` for core, cli and llm). It runs no `ts:test`, `go:test`, `rust-fe:test` or `rust-rt:test`.
- `.agent-plugins/bento/bento/land-work/verifier.json` has only `schema_version` and `command`, with no timeout.
- `.claude/skills/pre-completion/SKILL.md`: a grep for `property`, `PBT` or `up to date` finds nothing.
- Sessions since 2026-09-04 saw 65 `task ... is up to date` results in 24 sessions (finding agent-repo-11).

## Acceptance criteria

- [ ] `/pre-completion` (`.claude/skills/pre-completion/SKILL.md`) adds three rows:
  1. Gate evidence covers every language or crate in the diff, using a mapping table: shatter-core and shatter-cli → core:test-ignored/cli:test, shatter-ts → ts:test, shatter-go → go:test, shatter-rust → rust-fe:test, shatter-rust-runtime → rust-rt:test + rust-fe:test, shatter-llm → llm tests.
  2. Property-test adequacy per CLAUDE.md Completion Checklist item 2.
  3. Any `Task "X" is up to date` line for a required leaf means the leaf did not run. Re-run it with `--force` or a fresh `TASK_TEMP_DIR` before claiming it.
- [ ] `scripts/land_work_verifier.sh` tees each check's output to stdout/stderr (bento-rdtn.4 adoption). For each check it emits `executed`, parsed from Task's `is up to date` lines (bento-rdtn.6 adoption), and `wall_seconds`. It also either runs `task affected` for the landing diff, or has its header and CLAUDE.md corrected to say what it actually runs.
- [ ] `verifier.json` declares a timeout.
- [ ] A unit test wired into `meta` runs `run_check` against a stub command that prints `Task "cli:test" is up to date`. It asserts `executed: false` and that the output reaches stdout. The test fails on today's script and passes after.
- [ ] Proof: one real landing after the change shows per-check `executed` and `wall_seconds` in the verifier's final JSON line. Paste the line into the close reason.

## Suggested approach

Rewrite `run_check` to run the command through `tee` into a temp log, `grep -c 'is up to date'` the log to set `executed`, and time the run. Add the three `/pre-completion` rows with the mapping table. Decide with str-35vtk.24 whether the verifier runs `task affected` or one exact `task check`, and do not duplicate that work here.

## Out of scope

- The root-cause fix for meta-stage checksum poisoning (str-qwua7.3).
- The full receipt system (str-qwua7.2, str-35vtk.24/.25).
- bento-side verifier changes (already shipped in bento-rdtn.4 and bento-rdtn.6).

## Filer note (companion one-liner on str-qwua7.55)

Do not file a separate note for finding agent-repo-11, which duplicates open str-qwua7.55. Add this one line on str-qwua7.55 instead:

> Audit 2026-09-22 (agent-repo-11): the output-tee, per-check `executed`/`wall_seconds` (bento-rdtn.4/.6 adoption) and per-language evidence rows are tracked in <verifier-per-language-evidence>.

## Metadata

- Priority: P2. Type: task. Size: M.
- Labels: landing, quality-gates, agents, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none.
- Related: str-qwua7.55, str-35vtk.24, str-qwua7.2, bento-rdtn.4, bento-rdtn.6.
- Source findings: tests-ci-06 (verifier correction applied: the str-qwua7.4 example was dropped because the changed Go test was run directly), with agent-repo-11 as context. Draft shatter-agent/18.
