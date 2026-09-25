# Explore markdown report under-reports discovered paths ("0 path(s)" while the batch line says 2-3)

- Priority: P1
- Type: bug
- Labels: report,explore,ux,coverage
- Tracker action: new issue
- Related: str-9q1z (closed, a different conflation), str-1hnm (float probe), str-qwua7.57
- Source findings: audit 2026-09-22 goals-03, prior-19 (L6 confirmed). The same root cause is also in L5 findings core-02, cli-ux-15 and artifacts-03 (float-probe path accounting). If a separate core-engine issue is filed for those, link it with `blocks`, or merge the two issues.

<!-- body -->
## Problem
In default (random) explore mode, the markdown report's path count and rows omit paths the explorer found. The stderr progress line and the spec disagree with the report in the same run. Users are told a function has 0 or 1 behaviours when it has 3. The behavior-map cache written from the same result can also be empty, so `revalidate` inherits the error.

## Evidence (reproduced at HEAD, `--clean --no-cache`, fresh directory)
- `explore 01-arithmetic.ts:compareMagnitudes`: stderr `100 iters, 4 paths, 3/3 branches`; markdown `**2 path(s)**`, with sum-large and both-small rows missing. `--spec-out` from the same run has 4 classes.
- A trivial `g(x){ if (x>1) return 1; return 0 }`: batch line `2 paths, 1/1 branches`; report `**0 path(s)** · **100%** coverage`; `--spec` shows `Behavioral classes: 2`.
- safeDivide: stderr says 3 paths, report says 0. classifyHttpResponse: report 9, stderr 11.
- With `--concolic`, the report count matches the batch line.

## Current code facts
- `shatter-cli/src/render.rs:100-140` renders rows from `ObservationOutput.unique_paths` and `new_path_executions`, not from the accumulated path set.
- `shatter-core/src/explorer.rs:1212-1319`: the float probe inserts path hashes directly into `obs_state.seen_paths` (`:1279-1283`) and only calls `push_raw_result`, so probe-discovered paths never count as "new". The artifact shows `unique_paths` of 0 while `raw_results` holds the distinct branch paths.

## Acceptance criteria
- For TS `01-arithmetic.ts` (classifyNumber, compareMagnitudes), safeDivide and a 2-branch fixture in default mode, the report's path count equals the stderr batch-line count and the number of `--spec` classes.
- The behavior-map cache for these functions contains one behaviour per discovered path.
- A regression test asserts rendered path count == batch-line count == spec class count, in both random and concolic modes.

## Suggested approach
Route float-probe executions through the aggregator's normal observe/new-path accounting, or render rows from the merged accumulator (spec classes). Add the invariant test in the CLI integration tests.

## Scope
In: path accounting and rendering for explore. Out: the HTML "Paths Found" label showing branches_covered (separate finding artifacts-13), and resume keying.
