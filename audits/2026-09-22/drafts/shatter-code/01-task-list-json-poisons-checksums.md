# Meta test's `task --list-all --json` writes checksums for 39 tasks, so `task check` and CI skip every stage-2/3 test

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | quality-gates,ci,taskfile,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.3, str-qwua7.2, str-35vtk.8 |
| source findings | gates-01, tests-ci-01 |

<!-- body -->
## Problem

`task check` exits 0 in a fresh worktree while every stage-2 and stage-3 test leaf reports `Task "X" is up to date` and never runs. Root cause: the meta-stage unit test runs `task --list-all --json` with cwd=repo root; on go-task 3.50/3.53 computing the JSON `up_to_date` field writes `.task/checksum/*` for every task with `sources:`. Stage 1 runs meta, so stages 2/3 then see fresh checksums. This is the root cause the open P1 str-qwua7.3 asked for.

## Current code facts / evidence

- `scripts/test_affected_gates.py:203-211` (`test_every_emitted_gate_is_a_real_task`) runs `subprocess.run(['task','--list-all','--json'], cwd=ROOT)`.
- `Taskfile.yml:459` runs that test in `meta`; `meta` is a dep of `check-static` (`Taskfile.yml:546`), which runs before `check-unit`/`check-integration` (`Taskfile.yml:535-573`).
- Repro (go-task 3.50.0, git-archive copy of HEAD): `task --list-all` writes 0 checksum files; `task --list-all --json` writes 39 (core-test-ignored, go-test, ts-test, cli-test, rust-fe-test, conformance, parity, ...). After that `task sub:test` prints `is up to date` even after a source edit. `--dry` or omitting `--json` avoids the writes.
- Evidence: `audits/2026-09-22/gates/check.log:780-799,942` ('is up to date' lines); snapshot `audits/2026-09-22/gates/poisoned-task-cache-snapshot/`.
- Test added 2026-08-29 in 8ae14f13 / b12e3054 (str-35vtk.8).

## Acceptance criteria

- The meta test no longer mutates `.task/` (listing uses `--dry`, a throwaway `TASK_TEMP_DIR`, or parses Taskfile YAML).
- New regression test: snapshot `.task/checksum` before and after running the `meta` task; assert no new or changed entries.
- In a fresh worktree `rm -rf .task && task check` shows every stage-2/3 leaf executing (no `is up to date` lines for test leaves).
- str-qwua7.3 is closed referencing this issue and root cause.
- (Optional) upstream go-task issue filed describing the `--list-all --json` side effect; link recorded here.

## Suggested approach

Simplest fix: run the listing with `env TASK_TEMP_DIR=$(mktemp -d)` (or add `--dry`). Then add the checksum-snapshot regression test to `meta`.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: Executed-vs-cached receipts and CI guards (see draft 02 and str-qwua7.2); fixing any tests that fail once leaves actually run (draft 02).
- Size: S

## References

- Audit findings: gates-01, tests-ci-01 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.3, str-qwua7.2, str-35vtk.8
