# Explore auto-resume is keyed only on source fingerprint: --concolic / budget / seed changes silently return stale results labelled with the new mode

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | explore,resume,concolic,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-b2my.15, str-060a, str-8q1b4, str-jd0d1, str-9m9o3 |
| source findings | core-13, cli-ux-16, artifacts-01, goals-05, prior-18 |

<!-- body -->
## Problem

After any explore run, re-running with different options (`--concolic`, `--max-iterations`, `--no-cache`) resumes the prior results instead of exploring, and the report labels them with the new explorer (e.g. 'Explorer: concolic (Z3-backed)' on random results). Every engine comparison is silently wrong, and the walkthrough's concolic/spec steps replay the random step.

## Current code facts / evidence

- `shatter-cli/src/commands/explore.rs:1209-1233` `try_resume_function` matches only function_name, status==completed and deep_fingerprint.
- `explore.rs:5214-5260` prints `[info] [resumed] <fn> ... (prior run)`; `render.rs:138-140` prints the current explorer label.
- Repro: `explore 01-arithmetic.ts:classifyNumber` then `explore ... --concolic --max-iterations 7` → `[resumed] classifyNumber: 3 branches ... (prior run)`, stdout `Explorer: concolic (Z3-backed)`; a later `--spec --max-iterations 30` printed `Exploration: 100 iterations`.
- `--concolic --no-cache` across 26 TS files resumed all 26 (goals-runs/ts-all-concolic.err).
- Partial-resume sidecars can load concolic `hash_branch_path` covered_paths into a random run (different hash space).
- `demo/walkthrough.sh:120` uses one SHATTER_ARTIFACT_DIR for all steps; steps 8/9 log `[resumed]`.
- SPEC documents resume only for `scan --resume`; there is no `--no-resume`.

## Acceptance criteria

- Resume key includes a hash of result-affecting options: explorer mode, iteration/time budgets, seeds, mocks, setup, solver settings, engine/frontend fingerprint.
- A resumed result keeps and prints its original explorer label, and the report header says '(resumed from prior run; --clean to re-run)'.
- `[info]` logs why a resume was accepted or rejected.
- CLI test: random run then `--concolic` run re-explores; same options twice resumes.
- Walkthrough steps use separate artifact dirs or --clean; SPEC §2.1 documents explore resume.

## Suggested approach

Add an options hash to summary entries and resume sidecars; invalidate on mismatch.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: core-13, cli-ux-16, artifacts-01, goals-05, prior-18 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-b2my.15, str-060a, str-8q1b4, str-jd0d1, str-9m9o3
