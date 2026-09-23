# Area: Goal achievement (L5) — 2026-09-22 audit

Reviewer scope: does Shatter do what its own documents promise? Stated goals were
taken from SPEC.md §1 and §3, README.md, PLAN.md (the historical v2 thesis,
§4.3–4.4 and §6.3), and the downstream coverage goals recorded in project memory
(kapow, zolem, pickpackit, each ≥90% line coverage, set 2026-07-01..03).

Everything was measured on a release build of the audit worktree
(`16794cef`, `target/release/shatter`, `shatter-rust/target/release/shatter-rust`).
All run outputs are under `audits/2026-09-22/goals-runs/`. Downstream projects were
exported read-only with `git archive HEAD` into `goals-runs/{zolem,pickpackit}`.
Nothing was written into /home/ketan/project/*.

**Confound:** the host load average ran between 60 and 210 on 32 cores during
these runs, because other reviewers and gates were running at the same time.
Timeout findings are marked medium confidence for that reason. Findings about
coverage and correctness do not depend on timing: every function listed ran its
full iteration budget ("100 iters … (ok)").

## Overall grade: **C**

The engine does discover behavior in straightforward pure functions in all
three languages, and runs on real code without configuration (zolem
`internal/fixture`: 34/34 functions completed; pickpackit `web/src/domain`: 56/59
behavioral). The three promises that set Shatter apart are weaker than the
documentation says:

1. **Regression detection:** `diff` cannot read anything Shatter writes, and
   `revalidate` ignores return values.
2. **Trustworthy behavioral specs and reports:** the explore report drops
   discovered paths, multi-file JSON drops files, and preconditions are
   tautologies.
3. **Honest coverage on real code:** the line metric is inflated for Go scan,
   deflated for Rust, and resumed results mask engine changes.

The downstream ≥90% goals have not been measured since 2026-07-07, and the last
numbers were 18–28%.

| Goal (source) | Grade | One-line evidence |
|---|---|---|
| Discover paths and generate inputs without hand-written tests (SPEC §1, README) | B- | TS canonical examples reach 55.7% lines at 100 iterations (445/799); easy examples reach 100%, and unions, parsers and validators reach 10–40% |
| Concolic (Z3) execution beats random (PLAN thesis) | D | On 21 hard TS functions, `--concolic` gets 36.1% lines against 40.7% for default, stopping at 21–35 iterations; computeArea stays at 0/6 branches |
| Behavioral specs readable by humans and machines (SPEC §3.3, PLAN §4.3) | C- | Explore markdown drops paths (0 of 3 shown for safeDivide); multi-file JSON keeps only the first file; preconditions are `param[0] == -1` |
| Regression detection (SPEC §1 "regression snapshots", §2.6–2.7) | D | `diff` has no producer; `revalidate` exits 0 after a return value changed and even/odd were swapped; `spec-diff` works but reports the swap as a precondition change plus "inconclusive" |
| Behavior-map mocking (SPEC §3.2) | C | Wired in scan (`scan_orchestrator.rs:1636`). The cache is keyed by bare function name, so same-named functions collide |
| Coverage of real code (downstream ≥90% goals) | D | Last baselines were kapow 18.3%, zolem 28.0%, pickpackit 26.3%; nothing re-measured since 2026-07-07; the metric itself is unreliable (goals-06) |
| Three languages supported (SPEC §1.3) | B- | TS strong. Go works at low parallelism but fails 48/48 in explore at default parallelism under load. Rust runs the demo, a clear improvement since the 2026-09-04 audit (was 0%), but reports about 54% lines on fully covered functions |

## Positives worth preserving

- The prior audit's P1 about Rust walkthrough functions at 0% coverage no longer
  holds in the form reported. All four Rust demo functions now execute and
  `classify_number` finds all four outcomes (`goals-runs/rust-walk.md`).
- `spec-diff` correctly flags a changed postcondition (`[CHANGED] Class 1 — old:
  returns "zero" new: returns "nil"`) and exits 1 (`goals-runs/regress/`).
- Scan's include diagnostic is good: `--include 'internal/fixture/*.go' matched 0
  files. Patterns are evaluated relative to scan root … Try: --include '*.go'`.
- The scan's "Failure impact (line-weighted)" and "N function(s) not attempted
  (total budget 120s exhausted). Raise --timeout-total…" messages are honest and
  actionable.
- zolem's `internal/fixture` scanned cleanly: 34 completed, 0 failed, 2
  unsupported. `ParseExhaustAction` switch cases were all found (6/6 branches),
  which the 2026-06 random baseline could not do (str-ior1).
- The examples carry `EXPECTED BRANCHES` comments (37 of 66 standalone files),
  which is ready-made known-answer data for an effectiveness gate.

## Measurements

### Canonical examples: TypeScript (default explorer, `--max-iterations 100`)

`goals-runs/ts-all-default.md` / `.err`. The aggregate below comes from the
per-function lines in the markdown.

- 52 functions; 445/799 lines = **55.7%**.
- Below 40%: computeArea 10% (0/6 branches), matchRoute 12% (3/19),
  classifyStatus 13% (0/3), authorizeRequest 14% (2/14), negotiateLanguage 18%
  (2/12), validateJwt 19% (2/8), parseDotenv 29%, classifySecret 29%,
  validateEmail 30% (6/19), routeRequest 31%, loadOrDefault 33%, parseSemver 36%,
  processStateMachine 38%, evaluateRobotsPolicy 39%.
- Known-answer check against `EXPECTED BRANCHES` comments (files 01–05, whose
  comments parse): 24/39 expected outcomes appear in the report. `05-unions.ts`
  is 2/11.
- Root cause for computeArea: the TS analyzer widens the discriminant literal
  type to `str` in each object variant. The artifact's `analysis.params[0].type`
  is `union` of three `object`s whose `kind` field is `{"kind":"str"}`. The
  generator emits `{"kind":"true","radius":2.0}`, which matches no case.

### Canonical examples: concolic compared with default on the hard subset

`goals-runs/ts-sub-concolic.md` (fresh directory; 9 files, 21 functions, `-w 4`):

```
common 21  default (185/454, 40.7%)  concolic (164/454, 36.1%)
 89% ->  59%  06-nested-control-flow.ts:27-77 (classifyHttpResponse)
 12% ->   4%  10-path-router.ts:39-129 (matchRoute)
 14% ->  21%  07-auth-validation.ts:113-171 (authorizeRequest)
[batch] computeArea: 21 iters, 1 paths, 0/6 branches (concolic)
[batch] negotiateLanguage: 23 iters, 2 paths, 1/12 branches (concolic)
```

Concolic stops after 21–35 iterations on most functions and is not better
overall. Branch counts improved on validateEmail (13/19 against 6/19) and fell
on classifyHttpResponse (10/13 against 13/13).

### Canonical examples: Rust and Go

- Rust (`goals-runs/rust-walk.md`): classify_number 4/4 outcomes found but
  reported as **54% (7/13 lines)**. classify_string 50%, compute_stats 53% with
  6/6 branches. `parse_language_preference` has more than 6 rows that throw
  `input 1 deserialization failed: invalid value: integer '-998', expected usize`.
- Go, 18 files at default parallelism (`goals-runs/go-all-default.err`):
  `Error: explore: all 18 attempted target(s) failed (build_failed=0,
  runtime_failed=0, timed_out=43)`; each failure is `request timed out after
  30s`. A single file finishes in 2m51s, with one of two functions timing out.
  At `-w 2 --request-timeout 180` the functions complete (`goals-runs/go-sub.*`;
  ClassifyNumber 4 paths, 3/3 branches in 11.2s).

### Downstream: zolem `internal/fixture` (scan, `--parallelism 4`)

`goals-runs/zolem-fixture-default.{json,md,out}`: 34 completed, 0 failed, 2
unsupported, 91 branches, 45 covered (49.5%). Per-function line coverage reads
100% for nearly every function, including:

```
(*Loader).Load                 br 7/18  lines 15/15 (100%)
(*fixturesYAMLSelector).Select br 2/10  lines 5/5   (100%)
(*SequenceCounters).Step       br 2/6   lines 9/9   (100%)
(*wasmSelector).Select         br 0/12  lines 3/0
```

`Loader.Load` spans loader.go:87–153. The executed lines were
`88,89,93-95,99-101,104,108-110,138,145,152`. The loop body (lines 111–136) and
every error return were never executed, yet the report says 100%. See goals-06.

`source_set` puts all 10 files in `fixture_sample` and reports
`productionish_source_lines: 0`, because the production package is named
`fixture` (goals-16).

### Downstream: pickpackit `web/src/domain` (scan, `--parallelism 4`)

`goals-runs/pp-domain.{json,md,out}`: 95 functions discovered; 59 attempted, 56
behavioral, 3 error_only, 0 failed; 36 not attempted because the project's own
`shatter.config.json` sets `timeout_total: 120`. Completed-subset figures are
206/266 lines (77.4%) and 48/96 branches (50.0%). The whole-source denominator
is 2481 production-ish lines. Low-coverage functions include compareItems (1/6
branches), groupPackingItems (0/6), rankInterviewCandidates (1/8) and
effectiveLegTagIds (0/4).

### Downstream history (not re-run; from memory files, downstream changelogs and artifacts)

| Project | Goal set | Last measured | Last value | Source |
|---|---|---|---|---|
| kapow | 2026-07-01 | 2026-07-06 (resolver dir only) | 18.3% exec lines (whole, 07-01); 18.8% resolver dir | memory `project_kapow_shatter_advise_log.md` |
| zolem | 2026-07-03 | 2026-07-05 | 28.0% lines, 63.5% branches | `zolem/docs/shatter/CHANGELOG-for-advise.md` |
| pickpackit | 2026-07-02 | 2026-07-07 | 26.3% lines combined | `pickpackit/docs/shatter/baselines/20260707-*-all-summary.json` |

- The newest dated entry in either committed changelog is 2026-07-07.
- kapow never created its planned `docs/shatter/CHANGELOG-for-advise.md`.
- kapow's `shatter-artifacts/source-code.md` (2026-08-17) shows 99/597 files
  failed, with reasons including `execution adapter not supported by Go
  frontend: go/http-handler`, `Maximum call stack size exceeded` (a TSX page)
  and `failed to deserialize frontend response (33 bytes)`.
- Still-open levers from those logs: str-j49xg (P1 epic: adapter executions
  return empty coverage), str-la75 (P1 axum State<T>), str-jyxr (P1 Rust
  prefetch timeout), str-wfd2 (P2 cross-file Rust type resolution, called "the
  highest-leverage engine fix" in memory `project_shatter_rust_single_file_analysis.md`),
  str-bh9wu, str-4yc9w (P1).

### Effectiveness harnesses

| Harness | State |
|---|---|
| `~/project/holdout` | Last run 2026-04-09: `targets_ok 5, targets_failed 7`, `total_branches_covered 294 / total_branches 294` (this cannot be real), and 8 "errors" per target that are actually `unchanged (fingerprint match)` skips. Abandoned. |
| `~/project/shatter-effectiveness` | Design (1057 lines, 2026-08-27) and 13-task plan (1260 lines, 2026-08-30). About 15 commits built a docs-integrity landing gate. **No `bench/` code exists.** Last commit 2026-08-31. |
| in-repo `tests/broad-run-corpus`, `demo/gauntlet*` | They check denominators, artifact integrity and exit status, not coverage. The gauntlet allowlist has 17 permanent FAIL entries, unchanged since 2026-05-08. |

## Findings

(The same findings appear in the structured output. The IDs are `goals-NN`.)

1. **goals-01 P1 L5: `shatter diff` has no producer.** `Snapshot::from_behavior_map(s)` / `write_to_file` (`shatter-core/src/snapshot.rs:70-105`) are never called outside `snapshot.rs`, per `grep -rn` across the repository. `shatter diff` on an explore artifact returns `missing field 'created_at'` and exit 2. On `.shatter-cache/behavior-maps/classifyHttpResponse.json` it returns `missing field 'version'` and exit 2. QUICKSTART §5 shows `shatter diff snapshots/shipping.json current/shipping.json` without saying how to create either file. str-6k6.1 ("JSON snapshot export and comparison") was closed with the reason "Closed".
2. **goals-02 P1 L5/L2: `revalidate` misses output regressions.** Baseline was `classifyNumber` (default explore). The edit changed `return "zero"` to `"nil"` and swapped even/odd. `revalidate` printed `[drift] classifyNumber (expected drift)` twice, `4/4 behaviors confirmed.`, and exited 0. The JSON showed `verdict: confirmed` for input `[0]`, whose return value changed. `classify_verdict` (`shatter-core/src/revalidation.rs:91-122`) compares only the branch path and error severity, and counts `ExpectedDrift` as confirmed (`shatter-cli/src/commands/revalidate.rs:145,182`). SPEC §2.7 says revalidate compares "observed against cached behavior. Exit 0 = no regressions".
3. **goals-03 P1 L5/L6: the explore report hides discovered paths.** `goals-runs/paths/`, fresh run with `--clean --no-cache`: stderr shows `safeDivide: 100 iters, 3 paths, 3/3 branches` and `compareMagnitudes: … 4 paths`. Markdown shows `safeDivide **0 path(s)** · 88%` with no rows, and `compareMagnitudes **2 path(s)**` (sum-large and both-small missing). The spec for the same run has 4 classes for compareMagnitudes. Other cases: `classifyHttpResponse` reports 9 paths against 11 in stderr, and `a.ts:classify` reports 0 paths. The renderer uses `ObservationOutput.unique_paths` and `new_path_executions` (`shatter-cli/src/render.rs:100-140`), not the accumulated path set.
4. **goals-04 P1 L5: multi-file explore JSON keeps only the first file.** `explore 01-arithmetic.ts 02-strings.ts -o out.json --spec-out spec.json` writes only 01-arithmetic's functions to both files; `classifyString` appears 0 times. With a glob (`'*.ts'`, 26 files), `-o ts-all-default.json` wrote `{"file":"01-arithmetic.ts","functions":[],"status":"no_targets","no_target_reason":"unclassified"}`. Code: `file_spec_bundles.first()` at `shatter-cli/src/commands/explore.rs:6648` and `:6703`.
5. **goals-05 P1 L5: explore silently resumes stale results across explorer modes.** A `--concolic --no-cache` run after a default run printed `Found prior explore summary … [resumed] classifyNumber … (prior run)` for every function, and its output matched the default run function for function (both 445/799). Every function was labeled `*Explorer: concolic (Z3-backed)*`. Resume is keyed only on the source fingerprint (`try_resume_function`, `explore.rs:1212-1223`), not on explorer mode, budget or engine version. SPEC §2.1 does not document this implicit resume. kapow memory records the same trap ("runs silently RESUME prior results").
6. **goals-06 P1 L5: the line-coverage metric is not trustworthy across languages.**
   - Go scan: `(*Loader).Load` reports 7/18 branches but 15/15 lines (100%), even though lines 111–136 never ran. `reconcile_line_coverage` (`shatter-core/src/observe.rs:143-145`) takes `.max(covered)`, which hides an undersized denominator.
   - Rust: no `instrumentable_line_count` (`protocol/parity-matrix.yaml:867-874`), so fully covered functions report 50–54%.
   - This is the metric the downstream 90% goals were defined on. No open issue covers the Rust gap.
7. **goals-07 P2 L5: canonical examples are weak and the weakness is allowlisted rather than tracked.** TS examples: 55.7% lines; 24/39 known answers in 01–05; computeArea 0/6 branches because the TS analyzer widens the literal discriminant (evidence above). `demo/gauntlet-scan-allowlist.yaml` lists 17 FAIL entries with no issue IDs, has not changed since 2026-05-08, and has no ratchet. The walkthrough exercises only files 01, 02, 03, 04 and 18.
8. **goals-08 P2 L4/L5: the concolic explorer is not better than default.** On the 21 hardest TS functions it gets 36.1% against 40.7%, often stopping after 21–35 iterations. Memory records the same on kapow (2026-07-02: "ZERO coverage delta vs random"). str-ior1 ("re-baseline Zolem with --concolic") was closed with the reason "Closed" and no data.
9. **goals-09 P1 AGENT: downstream coverage goals are stalled and untracked.** No measurement since 2026-07-07 (2.5 months). No tracker epic owns the kapow, zolem or pickpackit goals: `.beads/issues.jsonl` has 0 open issues matching "coverage goal". kapow's committed change log was never created.
10. **goals-10 P2 AGENT: effectiveness measurement has been designed three times and never delivered.** See the harness table above.
11. **goals-11 P2 L2 (shatter-agents): the shipped `shatter-diff` skill documents a command that does not exist.** `catalog/skills/shatter-diff/SKILL.md:46`: `shatter diff [<base-ref>] [--staged] [--include-tests]`, and a pre-commit hook recipe at `:133`. Actual: `shatter diff <SNAPSHOT> <CURRENT>` (`args.rs:1429-1437`). The implementing epic str-81xiw is still open. The skill ships in plugin 0.1.12 (`plugins/claude/shatter/skills/shatter-diff`). This machine has 0.1.1 installed, which does not include it.
12. **goals-12 P2 L5: Go explore has no cold-build warmup gate.** Explore at default parallelism (16 workers, 1 session) timed out on 48/48 Go functions. zolem's package scan (`--parallelism 4`) completed 34/34. `BuildWarmupGate` exists only in `scan_orchestrator.rs` (str-tbk9e fixed scan only). Medium confidence because of host load.
13. **goals-13 P2 L5: spec preconditions are still single-sample equalities (prior finding holds).** In `goals-runs/regress/base.json`, class "negative" has `AllEqual{param_index:0,value:-1}` with `sample_count: 20`, and all provenance is `observed`. `spec-diff` reports the even/odd swap as `[PRECOND] param[0] == 2 → == 1` plus `[INCONCLUSIVE]`, not as a regression.
14. **goals-14 P2 L5: the behavior-map cache is keyed by bare function name.** `a.ts:classify` and `b.ts:classify`, explored together, produce one `.shatter-cache/behavior-maps/classify.json` that holds only b's behavior (`"b-neg"`). `path_for` uses the function_id (`cache.rs:291`). `revalidate` and mock reuse can therefore replay the wrong function.
15. **goals-15 P2 L5: Rust usize parameters get negative inputs** (prior str-qwua7.14, still open). The report lists deserialization failures as throws in the behavior table.
16. **goals-16 P3 L6: the source-bucket classifier treats production packages named `fixture` as fixtures.** zolem shows `fixture_sample: 10 files / 1279 lines` and `productionish_source_lines: 0`.
17. **goals-17 P3 L6: markdown reports contain raw NUL bytes.** `grep` treats `zolem-fixture-default.md` as binary; line 836 renders the input `"\u0000"`.
18. **goals-18 P3 L6: `explore --analyze-only` requires `--allow-host-writes`,** even though it executes nothing: `Error: refusing to execute target functions without a sandbox.`
19. **goals-19 P3 L3: `docs/stories/` does not exist** (prior str-qwua7.52, still open). There is no durable statement of user journeys, such as "baseline → change → detect regression", that would have exposed goals-01, 02 and 04.

## Biggest gaps between promise and reality

1. The regression-snapshot story (`diff`, `revalidate`) is mostly not there, even
   though SPEC §1 describes it as a core output. Only `spec-diff` works end to end.
2. The human-readable report and the machine-readable output silently lose data:
   dropped paths, and only the first file kept.
3. The coverage number that drives every downstream goal is not trustworthy:
   inflated for Go scan, deflated for Rust, and stale results are silently resumed.
4. The concolic claim: the Z3 explorer does not beat random on the project's own
   examples.
5. No living effectiveness measurement exists. Downstream goals stalled in July,
   and the benchmark repository contains only design documents.

## Agent-system root causes (cross-cutting)

- Gates measure exit status, artifact integrity and allowlists, not **effect**.
  Nothing asserts known answers, so goals-03, 04, 07 and 13 went unnoticed.
- Tracker hygiene: issues close with the reason "Closed" and no evidence
  (str-6k6.1, str-ior1), so a half-built feature (snapshot export) and a missing
  measurement (concolic re-baseline) look done.
- Goals live in agent memory files rather than in tracker epics or in-repo
  documents, so they stall with nothing to prompt anyone when a session ends.
- Parallel-path drift, which CLAUDE.md already warns about, persists between
  explore and scan: the warmup gate exists only in scan, and resume semantics
  differ. The warning has no mechanical check behind it.
- Skills in shatter-agents are published ahead of the engine features they
  document, with no test that the documented flags exist.
