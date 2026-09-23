# CI 'Full landing gate' has run no product tests since 2026-08-29: add an executed-leaf guard and triage what fails once it runs

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | ci,quality-gates,audit |
| parent | audit epic (draft 00) |
| blocked by | draft 01 |
| related | str-qwua7.2, str-35vtk.21, str-6nul9, str-k7czv |
| source findings | gates-02 |

<!-- body -->
## Problem

Because of the checksum poisoning in draft 01, the CI `task check` step has executed no unit, integration, E2E, conformance or parity tests since about 2026-08-29; `main` has been merged on a hollow green. Once draft 01 lands, real failures will surface and need triage, and CI needs a guard so a hollow pass cannot recur. The CLAUDE.md claims ('Full = Landing, CI'; snapshots verified in CI) are false until this is done.

## Current code facts / evidence

- `gh run view 35756993223 --log`: `Task "go:test" is up to date` ... `core:test-ignored`, `conformance`, `parity` all up to date; the only `test result:` lines come from the separate shatter-llm step.
- CI durations on main: 330-613 s (2026-08-10..08-27), 154-225 s from 2026-08-30 onward.
- Known failures when leaves are forced to run locally: str-k7czv (Go loader test, see draft 06), str-6nul9 (bench_frontier_ranking timeout), 10 TS handler timeouts under load (draft 07).
- CI has no cargo-nextest, so `core:test-ignored` falls back to `cargo test --include-ignored` with no per-test timeout; bench_frontier_ranking will run >240 s (see str-6nul9, draft 08).
- str-35vtk.21 ('CI runs full landing gate') was closed on run 33277936601; that guarantee no longer holds.
- `scripts/test_ci_workflow_structure.py` asserts workflow shape only.

## Acceptance criteria

- CI fails if the `task check` output contains `is up to date` for any test leaf (grep guard or fresh `TASK_TEMP_DIR` per run).
- A CI run on main after draft 01 lands executes all stage-2/3 leaves; every failure it surfaces is fixed or filed with an issue id recorded here.
- CLAUDE.md 'Test Tiers' / 'Code Quality Standards' claims are re-checked and left true (or corrected) once CI is real.

## Suggested approach

Land draft 01 first. Add a post-step in `.github/workflows/ci.yml` that greps the check log for `is up to date` on test leaves and exits 1. Re-run CI on main and triage.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: The executed-vs-skipped receipt system (str-qwua7.2) beyond the simple CI grep guard.
- Size: M

## References

- Audit findings: gates-02 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.2, str-35vtk.21, str-6nul9, str-k7czv
