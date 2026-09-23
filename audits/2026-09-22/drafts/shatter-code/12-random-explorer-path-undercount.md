# Explore report under-reports discovered paths (report '0 path(s)' / '1 path(s)' while the batch line and spec show 2-4)

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | explorer,report,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-1hnm, str-9q1z |
| source findings | core-02, cli-ux-15, artifacts-03, goals-03 (L6, same root), prior-19 (L6) |

<!-- body -->
## Problem

The random/hybrid explorer's user-visible path count and path rows are wrong. The float probe writes path hashes into `seen_paths` without counting them, so the main loop treats those paths as already seen; reports then show fewer paths than were found (sometimes 0 with 100% coverage). The behavior-map cache is also written with empty behaviors, so revalidate is affected.

## Current code facts / evidence

- `shatter-core/src/explorer.rs:1212-1319` float probe; `:1279-1283` inserts probe path hashes into `obs_state.seen_paths` and only calls `push_raw_result`.
- `shatter-cli/src/render.rs:100-137` renders `ObservationOutput.unique_paths` / `new_path_executions`, not the accumulated path set.
- Repro: `shatter explore mix.go --clean` → progress `Classify: 100 iters, 2 paths, 2/2 branches`, report `**0 path(s)** · **80%**`; artifact `00003_Classify.json` has unique_paths=0, raw_results=45.
- TS: `explore 01-arithmetic.ts:compareMagnitudes --clean --no-cache` → stderr 4 paths, report 2 rows; safeDivide stderr 3, report 0. fmt2 (n>10 big, n<0 neg, else throw): stderr 3 paths each run, report varies 0/1 while `--spec` shows 3 classes. Rust safe_divide 2 vs 1.
- Two same-named functions a.ts/b.ts: stderr 2 paths each, report 0, and `.shatter-cache/behavior-maps/classify.json` has `behaviors: []`.
- e2e_float_probe.rs asserts classification only.

## Acceptance criteria

- Probe executions go through the aggregator's normal observe/new-path accounting.
- Invariant test: rendered path count == progress-line path count == spec class count for known-answer fixtures (01-arithmetic TS, a float-param fixture, Rust safe_divide) in random mode.
- Behavior-map cache entries contain the discovered behaviors.
- Concolic path count on the same fixtures unchanged.

## Suggested approach

Route probe results through the aggregator; render rows from the merged accumulator.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: Path-identity unification between engines (draft 17).
- Size: S-M

## References

- Audit findings: core-02, cli-ux-15, artifacts-03, goals-03 (L6, same root), prior-19 (L6) (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-1hnm, str-9q1z
