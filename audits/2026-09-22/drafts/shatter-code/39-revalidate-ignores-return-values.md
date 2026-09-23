# `revalidate` ignores return values: changed outputs on replayed inputs are reported as confirmed and exit 0

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | regression,cli,behavior-map,audit |
| parent | audit epic (draft 00) |
| blocked by | draft 12 |
| related | str-kab3, str-3lob, str-76z6 |
| source findings | goals-02 |

<!-- body -->
## Problem

SPEC §2.7 says revalidate compares observed against cached behaviour and exits 0 only when there are no regressions. The verdict uses only branch path and error severity, and a path change under a code change counts as ExpectedDrift → confirmed, so an output regression passes.

## Current code facts / evidence

- `shatter-core/src/revalidation.rs:86-122` `classify_verdict` takes code_changed, path_matches and severity — never the return value.
- `shatter-cli/src/commands/revalidate.rs:145`, `:182` exit logic.
- Repro: explore 01-arithmetic.ts, change `return "zero"` → `"nil"`, run `shatter revalidate 01-arithmetic.ts` → every behavior `[ok] ... (confirmed)`, '6/6 behaviors confirmed.', exit 0. JSON verdict 'confirmed' for input [0].
- Unit tests only exercise the verdict matrix on synthetic severity/path inputs.
- Note draft 12: behavior maps can currently be written with empty behaviors.

## Acceptance criteria

- Return value and thrown error (with the nondeterminism mask) are part of the verdict; an output change on a replayed input is a regression regardless of fingerprint.
- ExpectedDrift does not count as confirmed for the exit code (or `--fail-on-drift`, default on in CI mode).
- E2E known-answer test: mutate a return value, revalidate exits 1 and names the input.

## Suggested approach

Extend Verdict with output comparison; reuse the nondeterminism mask from str-kab3.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: goals-02 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-kab3, str-3lob, str-76z6
