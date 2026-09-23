# CI runs plain `cargo test` (nextest `[profile.ci]` is dead config) and parity-governed keeps a stale 'pending str-7jgm.2' fallback

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | chore |
| priority | P3 |
| labels | ci,quality-gates,audit |
| parent | audit epic (draft 00) |
| blocked by | draft 01 |
| related | str-6nul9, str-35vtk.7 |
| source findings | gates-08, tests-ci-07 |

<!-- body -->
## Problem

Local gates use nextest when installed; CI never installs it, so `.config/nextest.toml [profile.ci]` (retries=1, fail-fast=false) never applies, the default profile's fail-fast=true hides tests after one failure (87/3531 unrun when bench_frontier_ranking times out), and the E2E suites have no per-test timeout in CI. Separately, `parity-governed` still has a fallback for a script that now exists.

## Current code facts / evidence

- `.github/workflows/ci.yml`: no cargo-nextest install, no `NEXTEST_PROFILE`.
- `.config/nextest.toml`: `[profile.default] fail-fast=true, terminate-after=2`; `[profile.ci]` unreferenced anywhere.
- `shatter-core/Taskfile.yml:28-33` falls back to `cargo test -- --include-ignored` when nextest is absent.
- `Taskfile.yml:271-275` parity-governed: `if [ -f scripts/validate-parity.py ] ... pending str-7jgm.2`; str-7jgm.2 is CLOSED and the script exists.

## Acceptance criteria

- CI installs cargo-nextest (e.g. taiki-e/install-action) and runs with `--profile ci`, or `[profile.ci]` is deleted with a comment explaining why.
- The landing gate uses `--no-fail-fast` or `--max-fail` so one timeout does not hide the rest of the suite.
- The stale fallback branch in parity-governed is removed.

## Suggested approach

Prefer installing nextest in CI (gives per-test timeouts once draft 01 makes CI real).

## Scope

- In scope: the acceptance criteria above.
- Out of scope: Excluding bench_frontier_ranking from the gate (str-6nul9).
- Size: S

## References

- Audit findings: gates-08, tests-ci-07 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-6nul9, str-35vtk.7
