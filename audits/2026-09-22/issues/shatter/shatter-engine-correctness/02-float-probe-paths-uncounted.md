---
slug: float-probe-paths-uncounted
kind: new
title: "Random explorer float probe marks paths seen without counting them; explore report says '0 path(s)' while progress line and spec show 2-4"
priority: P1
type: bug
labels: [explorer, report, coverage, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Random explorer float probe marks paths seen without counting them; explore report says '0 path(s)' while progress line and spec show 2-4

## Problem

In default (random/hybrid) explore mode, the path count and path rows in the explore report are wrong. The float-probe pre-pass writes its path hashes straight into `seen_paths` without counting them as new paths. The main loop then treats those paths as already seen. As a result the markdown report shows fewer paths than the explorer found, sometimes "0 path(s)" at 100% coverage. The stderr progress line and `--spec` from the same run show the right number.

The behavior-map cache written from the same result can also have `behaviors: []`, so `revalidate` inherits the error. Users are told a function has 0 or 1 behaviors when it has 3.

With `--concolic`, the report agrees with the progress line.

## Evidence

Code (line numbers re-checked on the audit branch, whose code is identical to `56c86168`):

- `shatter-core/src/explorer.rs:1212-1319` is the float probe. At `:1279-1283` it computes `path_hash` for the float and floor executions and calls `obs_state.seen_paths.insert(...)` directly. At `:1297-1301` it calls only `aggregator.push_raw_result(...)`, so probe paths never enter `unique_paths` / `new_path_executions`.
- The probe runs whenever `n_float * PROBE_COUNT * 2 < max_iterations` (`:1214-1216`).
- `shatter-cli/src/render.rs:100-137` renders `**{} path(s)**` from `result.unique_paths` (:106, :109) and rows from `new_path_executions` (:113), not from the accumulated path set.
- `shatter-core/tests/e2e_float_probe.rs` asserts only the probe classification, never user-visible path counts.

Observed during the audit (`--clean`, and `--no-cache` where noted, fresh directory):

- Go `Classify(x float64)`, `shatter explore mix.go --clean`: the progress line said `[batch 1/2] Classify: 100 iters, 2 paths, 2/2 branches`, but the report said `**0 path(s)** · **80%**` with no rows. Artifact `00003_Classify.json` had `unique_paths=0`, `new_path_executions=[]`, `raw_results=45`, and float_probe classification `integer_treating`. The concolic engine reports 2 paths for the same function.
- TS `explore 01-arithmetic.ts:compareMagnitudes --clean --no-cache`: stderr `100 iters, 4 paths, 3/3 branches`, markdown `**2 path(s)**`. The sum-large and both-small rows were missing, and `--spec-out` from the same run had 4 classes (goals-03).
- TS `safeDivide`: stderr `3 paths, 3/3 branches`, report `**0 path(s)** · 88%`, or `**1 path(s)**` in another run. The artifact's `raw_results` held 3 distinct branch paths (10/51/49 executions) with `unique_paths` 0 or 1 (goals-03, cli-ux-15). `classifyHttpResponse`: report 9, stderr 11.
- A trivial TS `g(x){ if (x>1) return 1; return 0 }`: batch line `2 paths, 1/1 branches`, report `**0 path(s)** · **100%**` and `Summary: 0 path(s)`, while `--spec` shows `Behavioral classes: 2` (prior-19).
- TS `fmt2` (n>10 'big', n<0 'neg', else throw), three runs of `explore --clean --max-iterations 30`: each printed `fmt2: 30 iters, 3 paths, 2/2 branches`. The reports showed 1 path (throw row only), 1 path ('big' only) and 0 paths at 100% coverage, and the spec in the same stdout said `Behavioral classes: 3` (artifacts-03). The output varies from run to run.
- Two same-named TS functions in `a.ts`/`b.ts`: stderr said 2 paths each, the markdown said 0 for both, and `.shatter-cache/behavior-maps/classify.json` had `behaviors: []` (goals-03).
- Rust `safe_divide`: stderr 2, report 1 (cli-ux-15).

## Acceptance criteria

- [ ] Float-probe executions go through the aggregator's normal observe/new-path accounting. No code path inserts into `seen_paths` without also recording the path as discovered.
- [ ] A regression test asserts rendered report path count == progress-line path count == `--spec` class count, in both random and concolic modes, on these known-answer fixtures:
  - From the external examples repo (`github.com/shatterproof-ai/examples`, resolved through `SHATTER_EXAMPLES_DIR` or `<tmp>/shatter-examples-main/standalone/` via `scripts/examples_checkout.py`): `standalone/ts/01-arithmetic.ts` (`classifyNumber`, `compareMagnitudes`), `standalone/ts/04-errors.ts` (`safeDivide`), and `standalone/rust/04_errors.rs` (`safe_divide`).
  - Self-contained, written as inline source strings to a tempdir by the test (no examples-repo change): a trivial 2-branch TS function (`g(x){ if (x>1) return 1; return 0 }`), the fall-through-throw TS `fmt2` shape (`n>10` returns 'big', `n<0` returns 'neg', otherwise throws), and a Go `Classify(x float64)` with nested `x > 0.5` / `x < 1`.
  - At close, quote the test output showing each fixture failing on current `main` and passing after the fix. If the test is `#[ignore]`d because it spawns frontends, run it with `-- --include-ignored` (or through the `task e2e-*` target) and quote the lines listing the test names as run; a run that skips it does not count.
- [ ] The behavior-map cache for these fixtures has one behavior per discovered path. A test asserts it, including the two-same-named-functions case (two self-contained TS files `a.ts` and `b.ts` that each define `classify`).
- [ ] `e2e_float_probe.rs` also asserts `unique_paths` / `new_path_executions` for a float-param fixture in random mode.
- [ ] The concolic path counts on the same fixtures do not change.
- [ ] `task affected` passes with its `Gates selected` output recorded, and `task e2e` passes (explorer change).

## Suggested approach

In the probe, replace the direct `seen_paths` inserts plus `push_raw_result` with the aggregator's normal observe call, so probe results count as new paths when they are new. Separately, consider rendering report rows from the merged accumulator, or from spec classes, so the report cannot drift from the engine's own counts. Add the three-way invariant test to the CLI integration tests.

## Out of scope

- Unifying path identity between the two engines (engine-path-identity-budget-config, bucket shatter-concolic-and-engine-design).
- The HTML "Paths Found" label that shows branches_covered (artifacts-13, tracked elsewhere).
- Resume keying (explore-resume-options-key).

## Priority

P1: the main user-facing report contradicts the engine's own output and hides discovered behaviors.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-1hnm (closed; introduced the float probe), str-9q1z (closed; a different branch_count/unique_paths conflation), str-4o07, str-qwua7.57.

## References

Audit 2026-09-22 findings core-02, cli-ux-15, artifacts-03, goals-03 and prior-19, all one root cause (report §14 item 5: "file once"). Source drafts: `drafts/shatter-code/12-random-explorer-path-undercount.md` (kept) and `drafts/shatter-docs-ui/02-explore-report-underreports-paths.md` (merged into this issue and not filed separately).
