# `--seed` exists only on scan; str-0m0vn closed although its symptom named explore

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | feature |
| priority | P2 |
| labels | cli,seeds,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-0m0vn, str-v1tzz, str-qwua7.20.1 |
| source findings | cli-ux-08 |

<!-- body -->
## Problem

str-0m0vn ('no way to make shatter scan or shatter explore reproducible') wired `--seed` into scan only. explore and run have no seed flag.

## Current code facts / evidence

- `shatter-cli/src/args.rs:970-979` defines --seed on Scan only; `explore --help` / `run --help` have no --seed.
- Scan-only follow-ups exist: str-pbqyr, str-9m9o3 (scan cache ignores seed), str-v1tzz (report seed).

## Acceptance criteria

- `explore --seed N` and `run --seed N` exist and thread into both engines' configs.
- Reproducibility test (like scan_seed_reproducibility) for explore: same seed + source ⇒ identical path set and inputs.
- Resume key (draft 22) includes the seed.

## Suggested approach

Add via the shared options struct (str-qwua7.20.1) if it lands first.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: cli-ux-08 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-0m0vn, str-v1tzz, str-qwua7.20.1
