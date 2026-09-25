# Core engine review (shatter-core) — 2026-09-22

Reviewer area: `core-engine` (L1 code quality, L4 design, L5 goal achievement).
Tree: `audit-2026-09-22` worktree @ `16794cef` (main). Observation only; nothing fixed.
Behavioural probes used the primary checkout's `target/debug/shatter` (built 2026-09-22 10:33,
same day as HEAD; the only later merge, str-qwua7.4, touches HTTP-body literal ValuePlans, not
the code paths probed) against a scratch Go module in the session scratchpad
(`gomix/mix.go`, reproduced below). `--allow-host-writes` was used because no sandbox backend
is configured; the scratch dir is disposable.

```go
package gomix
func Classify(x float64) string {
	if x > 0.5 {
		if x < 1 { return "low" }
		return "high"
	}
	return "neg"
}
func Loopy(n int) int {
	s := 0
	for i := 0; i < n && i < 50; i++ { s += i }
	if s > 100 { return 1 }
	return 0
}
```

## 1. Component status

| Component | Status | Notes |
|---|---|---|
| Random/hybrid explorer (`explorer.rs`, `explore_function` 965 lines) | **Partial** | Float-probe phase hides paths (F2); shrink runs after function teardown with `setup_context: None` (F9); path identity = scope-aware + loop buckets. |
| Concolic orchestrator (`orchestrator.rs`, `explore_with_oracle` 1,372 lines, up from 1,307 at the last audit) | **Partial** | Ignores `--setup` in every production caller (F3); dynamic mock variation regressed (F4); refine phase drops `prepare_id`/`execution_profile` and discards the paths it reaches (F6); raw branch-path identity makes loops explode path counts (F7); capture still hard-coded (str-qwua7.5). |
| Z3 translation (`solver.rs` `to_z3_expr` & co.) | **Partial, unsound for mixed Int/Real** | One parameter can become two unrelated Z3 variables (F1); float constants use ×1e6 i64 scaling (F13); no default query timeout (F12). |
| Model extraction (`extract_concrete_values`) | **Partial** | Iterates ints, then reals, bools, strings into one map keyed by name, so a later sort silently overwrites an earlier one (F1); Int values beyond i64 are dropped silently. |
| Behavior maps / equivalence (`behavior.rs`, `equivalence.rs`) | **Solid** (not re-verified in depth; prior audit's assessment stands) | Spec classes line up 1:1 with eq classes by construction (`spec.rs:342-346`). |
| Invariants (`invariants.rs`) | **Partial** | Function-level invariants are emitted from any number of specimens, including 1; the `confidence` field is always 1.0; only `>0`, `>=0`, `<0` and constant numeric templates (F11). |
| Input generation (`input_gen.rs`, 9,025 lines) | **Solid, but carries a dead duplicate shrinker** (F5) | |
| Shrink (`shrink.rs`) | **Partial** | The live value shrinker lacks 3 fixes that went into the dead copy (F5). The generic `shrink_witness` driver is test-only, while production runs two hand-inlined copies (str-qwua7.6.1). |
| Scan orchestrator (`scan_orchestrator.rs`, 15,251 lines) | **Partial** | Sequential `scan()` (482 lines) has no production caller (F8). An empty worker pool gets reported as a per-task timeout (F10). Its concolic config has different budget semantics from explore (F7b). |
| Harness storage (`harness_storage.rs`) | **Solid** | Clear three-root lifecycle split, small, has tests. |
| `recursive.rs`, `array_mutation.rs`, `reporter.rs` + `clustering.rs`, `export.rs` | **Dead / unwired** | About 4,670 lines of `pub` modules with no production caller (F8). Only `export.rs` is tracked (str-qwua7.59). |

## 2. Status of prior-audit items assigned to this area

| Issue | State | Verified today |
|---|---|---|
| str-qwua7.5 capture_side_effects in concolic | OPEN, **still true** | `orchestrator.rs` still has no field. Execute sites hard-code `capture:` at 1694, 2380, 2701, 2714, 3103, 3643, 3687, 3741 (line numbers have moved since the issue was filed). **Correction to the issue:** the random explorer does not fully honour the flag either. Its shrink pass hard-codes `capture: true` at `explorer.rs:1779` and `false` at 1824 and 1876. Only `observe_single` (`:1573`) and the observer-pool worker (`:2784`) honour it. |
| str-qwua7.6 dedup epic | OPEN, **still true** | The shrink selection block is still duplicated (`explorer.rs:1700-1740` vs `orchestrator.rs:3561-3600`). `explore_with_oracle` has **grown** from 1,307 to 1,372 lines. There are now **three** hand-written `orchestrator::ExploreConfig` literals, not one: `shatter-cli/src/commands/explore.rs:5138`, `shatter-core/src/scan_orchestrator.rs:3080`, and `shatter-cli/src/commands/observe.rs:107`. |
| str-qwua7.29 module cycles | OPEN, **still true** | `orchestrator.rs:34 use crate::explorer::{apply_live_first_overrides, update_live_first_states}` against `explorer.rs` importing `crate::orchestrator::hash_branch_path`. `explorer.rs:175,178` reference `crate::scan_orchestrator::{BudgetSurplus, ClaimPolicy}`. `input_gen` and `strategy` both import `crate::orchestrator`. |
| str-qwua7.30 unwrap_used / missing_docs lints | OPEN, **still true** | `shatter-core/Cargo.toml:46-47` has only `[lints.rust] unexpected_cfgs`. `lib.rs` has no `#![warn(missing_docs)]`. |
| str-qwua7.47 proptests for untested modules | OPEN, **still true** | `proptest!` count is 0 in frontend.rs, string_mutation.rs, array_mutation.rs, invariants.rs, behavior.rs, call_graph.rs and core_sample.rs. **Scope correction:** array_mutation.rs has no production caller (F8), so writing property tests for it is wasted work. Delete it instead. |
| str-qwua7.49 scan pool expects | OPEN; **the premise is partly wrong** | The code at `:2244`/`:2254` is `sender.send(fe).await.expect(..)`, not `try_send`. `checkout()` at `:2270` can never observe `None`, because `WorkerPool` keeps its own `sender` alive (`shutdown()` drops it explicitly at `:2378`). The reachable failure is different. When every replacement spawn fails, `replace_dead_worker_if_needed` → `reap_dead_slot` (`:2317-2327`) drives `live_count` to 0. Queued tasks then block in `checkout()` until the join watchdog fires, and each one is reported as a task timeout (see F10). |

## 3. Findings (new unless prior_ref given)

### F1 [P1, L5/L1] Z3 translation splits one numeric parameter into two unrelated variables

- `solver.rs:500-503`: the sort of a `Param` is `declared` only when the declared and hint sorts are *incompatible*. Int and Real count as compatible (`sorts_compatible`, `:127-135`), so the per-comparison hint wins.
- `to_z3_real` calls `to_z3_expr(.., Sort::Real)` and `to_z3_int` uses `Sort::Int` (`:951-981`). `VarTable` keeps separate `ints` and `reals` maps keyed by the same name (`:73-121`).
- Result: for `x: float64` with path `x > 0.5` (Real) and `x < 1` (Int const → Int comparison), Z3 receives a Real `x` and an Int `x` with no link between them. `extract_concrete_values` (`:1023-1066`) inserts ints first and then reals into one `HashMap`, so the Real value silently overwrites the Int one. The model it returns does not satisfy the negated path.
- Integer range bounds (`assert_int_param_ranges`, `:172-181`) only bind the Int variable. An Int param compared with a float constant gets an unbounded Real twin.
- Evidence from the Go frontend: the branch constraints recorded in the artifact are `{"op":"gt","right":{"type":"float","value":0.5}}` and `{"op":"lt","right":{"type":"int","value":1}}`. Running `shatter explore mix.go --concolic --max-iterations 40 --clean` gives `Classify: 23 iters, 2 paths ... worklist_exhausted`, with 80% line coverage and `return "low"` never reached. The inputs tried were {-3.14, -2.72, -2, -1, -0.5, -0, 0, 0.5, 1, 1.5, 2, 2.72, 3.14}; none lies in (0.5, 1). The default random/hybrid run (100 iterations, which includes the `Z3Solver` strategy calling the same solver) also never reaches "low".
- **Direct unit reproduction.** A scratch binary (`scratchpad/solverrepro`) depends on shatter-core by path and calls the public `shatter_core::solver::solve_for_new_path` with `x: TypeInfo::Float` and constraints `[x > 0.5 (Float const), NOT(x < 1 (Int const))]`, negating index 1. The intended solution is 0.5 < x < 1. Across three runs it returns `Ok(Sat({"x": Float(1.5)}))`, which violates the negated constraint and is reported as SAT.
  - Second case: `n: u8` with `n < 1000.5`, negating `n > 300.5`. It returns `Ok(Sat({"n": Float(0.0)}))`, a float for an unsigned-int param. The Real twin won and the u8 range bound was not applied to it. The `#[ensures]` type check accepts this because Int and Float count as interchangeable.
- This is the core promise of the concolic engine: flipping a two-branch float path should be trivial.
- The prior audit graded the solver "Partial (deliberately) … good engineering" and did not catch this. str-r6fr (closed) added proptests for the solver bridge, but none mixes sorts on one parameter.
- Recommendation: decide one Z3 sort per variable up front, from `param_sorts`, and coerce with `to_real`/`to_int` at use sites instead of declaring a second variable. Add a unit test (`x: Float`, constraints `x > 0.5` Real and `x < 1` Int, negate idx 1 → expect 0.5 < x < 1) and a Go/TS E2E known-answer fixture shaped like `Classify`. Also assert in `extract_concrete_values` that a name never appears in two sort tables.

### F2 [P1, L5/L6] Random explorer's float-probe phase hides discovered paths: report says "0 path(s)"

- `explorer.rs:1279-1283`: the float probe inserts its path hashes straight into `obs_state.seen_paths` and pushes a raw result (`:1297`). It never records a new-path execution.
- When the main loop later hits those paths, they are already "seen", so `unique_paths` stays 0 and `new_path_executions` stays empty.
- The probe runs whenever `probe_budget = n_float × PROBE_COUNT(5) × 2 < max_iterations`, which covers any function with a float param at the default 100 iterations.
- Evidence: `shatter explore mix.go --clean` (default budget) prints the progress line `[batch 1/2] Classify: 100 iters, 2 paths, 2/2 branches`. The final report then says `**0 path(s)** · **80%** coverage` with no example table, and the summary counts 0 for Classify. The artifact `00003_Classify.json` has `unique_paths: 0`, `new_path_executions: []`, `raw_results: 45`, and `float_probe_results[0].classification: integer_treating`.
- The concolic probe (`orchestrator.rs` ~2660-2730) does not have this bug, so the two engines disagree for the same input.
- Recommendation: route probe observations through the aggregator's normal `observe`/new-path accounting, not raw `seen_paths` writes. Add an E2E assertion on `unique_paths`/`new_path_executions` for a float-param fixture in random mode. `tests/e2e_float_probe.rs` only checks the classification.

### F3 [P1, L5/L4] `--setup` is silently ignored under `--concolic` (setup parity is plumbing only)

- `orchestrator.rs` contains no `send_setup` or `teardown` call (`grep -c teardown orchestrator.rs` = 0). It relies on callers passing `setup_context`.
- All three production callers pass `None`:
  - `pipeline_orchestrator.rs:536-548` (explore CLI concolic)
  - `scan_orchestrator.rs:3103-3115` (scan concolic)
  - `shatter-cli/src/commands/observe.rs:179-190`
- str-0s76.6 was closed with "orchestrator::explore() now accepts and threads setup_context … matching explorer behavior". The E2E test `e2e_concolic.rs:1555 orchestrator_explore_with_setup_context` hands a pre-built context directly to `orchestrator::explore`, so it passes while the real pipeline never builds one.
- The CLI prints no warning when `--setup` and `--concolic` are combined (no hit for setup+concolic in `commands/explore.rs`).
- Recommendation: make the pipeline layer own setup and teardown for both engines. One helper should send Setup, pass the context, and send Teardown, called from `run_pipeline`/`explore_with_scan_mode`/observe. Add an E2E test through `pipeline_orchestrator`/CLI (not the orchestrator API directly) that asserts the setup side effect is visible in concolic executions.

### F4 [P1, L5] Concolic dynamic mock variation regressed; the regression is hidden by `_`-prefixed names

- str-3ky9.4 "Orchestrator dynamic mock variation" was closed on 2026-03-10 (e0313728). On 2026-03-14, 0293c35c (str-lebv, "wire MetaStrategy into orchestrator") dropped it.
- `orchestrator.rs:2640-2647` now generates `_initial_mocks` and discards them, with the comment "Retained for future use; currently unused".
- `solve_and_generate` takes `_mock_params: &[MockParam]` (`:2147`) and ignores it.
- `input_gen::mutate_mock_values` now has no production caller (only tests at `input_gen.rs:8081, 8533`).
- The random explorer still draws fresh mock values every iteration (`explorer.rs:1536-1537`, `2467-2468`). Concolic therefore cannot explore branches that depend on mocked return values, and the discarded `generate_mock_values` call still consumes RNG draws.
- Recommendation: restore mock variation in the MetaStrategy concolic loop, or explicitly document and warn that concolic mode fixes mocks. Delete the `_initial_mocks` dead block. Add a known-answer E2E (a branch on a mocked dependency's return value) for concolic.

### F5 [P2, L1/L5] Two divergent value shrinkers; fixes landed in the dead copy and a closed issue claims otherwise

- The live shrinker is `shrink.rs:363 shrink_candidates`, called from `explorer.rs:1850` and `orchestrator.rs:3716`. A second, dead `pub fn shrink_candidates` lives at `input_gen.rs:3574`; its only callers are its own recursive helpers.
- Fixes that exist only in the dead copy:
  - str-ddxe (3d458b2a): range-aware int shrinking. `input_gen.rs:3596-3619` never proposes -1 for unsigned types, while `shrink.rs:395-412` always emits -1.
  - str-55ep (f40facf1, touching `input_gen.rs` only): drop only optional fields. `shrink.rs:512-539` removes every field, including required ones.
  - Positional (tuple) objects: `input_gen.rs:3752`. `shrink.rs` returns nothing for them because `value.as_object()` is None for a JSON array.
- str-55ep's close reason says "shrink_object now drops only optional fields", and its description names "missing field id is a shrinker artifact". That artifact is still produced. It is partly masked because `planner_consumer.rs:317-330` (str-kn3f) repairs required fields before execute. The accepted shrunk witness (`current = bulk_trial`, unrepaired) is then stored and displayed without the required field, so the witness users see is not what executed.
- Open issue str-v0yjq asks for enum-domain shrinking in `input_gen.rs` `shrink_candidates`/`shrink_union` and in `recursive.rs`. Both are dead paths, so the planned work would have no effect.
- Recommendation: delete `input_gen::shrink_candidates` and its helpers after porting the three fixes into `shrink.rs`. Retarget str-v0yjq to `shrink.rs`. Store the repaired inputs as the witness. Add a proptest that `shrink_candidates` output stays inside `int_range` and keeps required fields.

### F6 [P2, L5] Concolic refine phase drops `prepare_id` and `execution_profile` and throws away the paths it reaches

- `orchestrator.rs:2374-2386`: boundary-refinement executes send `prepare_id: None, execution_profile: None`. This means a rebuild per execute on Go/Rust prepare-capable frontends, and TS adapters such as `ts/react-hooks` are dropped.
- The refine results only set `tw`/`fw` witnesses (`:2403-2407`). They never feed `covered_paths` or `raw_results`, so paths first reached during refinement are not reported.
- Evidence: the concolic `Classify` artifact has `boundary_results[0].true_witness: [0.5000037571385455]`, 20 executions. That input takes `x>0.5` and then `x<1` → "low", yet the report shows 2 paths and 4/5 lines.
- Recommendation: thread `prepare_id`/`execution_profile` like every other execute site, and do it through a single `execute_request()` builder (str-qwua7.6 scope). Feed refine executions through the same observation accounting as the main loop.

### F7 [P2, L4/L5] The two engines define "path" differently; concolic path counts explode on loops

- The random explorer uses `explorer::path_hash` (`:566-571`): scope-aware plus loop buckets (`LoopBuckets`), with a fallback to lines/error/return for branch-free functions.
- Concolic uses `orchestrator::hash_branch_path` (`:864-871`), a raw sequential hash of `(branch_id, taken)`. Every distinct loop trip count is a new "path", and all branch-free executions collapse into one.
- Evidence (40 iterations each): `Loopy` gives 16 "paths" in concolic and 6 in random for a function with 2 behaviours. The concolic report lists 16 rows. Concolic `max_iterations` is a *unique-path* cap (`observe_one` `:1621-1627`), so loops use it up.
- F7b: `--max-iterations` also means different things per entry point:
  - `explore --concolic` sets `max_executions = max_iterations` (`commands/explore.rs:5145`, str-nqrz).
  - `scan --concolic` uses `concolic_scan_max_executions` = 5× (`scan_orchestrator.rs:3144-3150`).
  - `observe --concolic` uses 5× (`observe.rs:109`).
  - The flag help for all of them says "Maximum number of iterations per function".
- Recommendation: move path identity into one module (`path_identity.rs`) used by both engines and the shrinker. Replace the three hand-built orchestrator configs with one `From<&explorer::ExploreConfig>` (folds into str-qwua7.6.2 / str-qwua7.20.1), with one documented budget meaning.

### F8 [P2, L1] About 5,100 lines of production-dead code in shatter-core, mostly untracked

No non-test caller in shatter-core/src, shatter-cli/src, shatter-llm/src or shatter-core/tests:

- `recursive.rs` (675 lines; `explore_recursive`, `explore_mutual_group`)
- `array_mutation.rs` (397)
- `reporter.rs` (1,326) plus `clustering.rs` (530, used only by reporter)
- `export.rs` (1,743, tracked as str-qwua7.59)
- `scan_orchestrator::scan()` (482 lines at `:1480`; sole caller is the test at `:7510`) and `build_summary_from_scan_result` (`:1225`, which says it serves "the non-parallel scan() path")
- `input_gen::shrink_candidates` family (F5)
- `shrink::shrink_witness` (test-only driver)
- `mutate_mock_values` (F4)

The prior audit graded clustering "Solid" and asked for proptests on array_mutation and expect-fixes in recursive.rs, all on dead code. str-8q1b4 (in progress) cites "sequential path :1583-1594" of the dead `scan()` as a fix site.

Recommendation: delete them, or wire them in with an issue. Add a gate that fails on `pub` items unreachable from the CLI binary; a cheap version is a script that lists `pub mod` entries with zero `crate::m::`/`shatter_core::m::` references outside the module and its tests.

### F9 [P2, L5] Random explorer shrinks witnesses after function teardown and without setup context

- Per-function teardown runs at `explorer.rs:1676-1683`, before the shrink pass at `:1696`. Shrink executes with `setup_context: None` (`:1778, 1823, 1875`).
- Witnesses found under setup state are re-executed without it. Their branch path usually differs, so shrinking burns budget and is rejected, or it accepts a witness whose outcome depends on state that is now missing.
- Both copies came from 4d8001bc9 (str-28ea.6). The orchestrator copy passes `setup_context.clone()` (`:3642`), so the "parity" direction is inverted.
- Recommendation: shrink before teardown with the live context, as part of the shared `shrink::select_witnesses` extraction.

### F10 [P2, L1] Empty worker pool shows up as per-task watchdog timeouts, not a spawn failure

See the str-qwua7.49 correction above (`scan_orchestrator.rs:2268-2271, 2317-2327, 4608`). When every frontend respawn fails, e.g. a frontend binary broken mid-scan or resource exhaustion, each remaining task waits its full `shared_pool_task_watchdog` and is reported as `phase_timeout_reason("task", …)`. The user never learns that the frontend could not spawn.

Recommendation: track `live_count == 0 && pending > 0` and fail fast with `ScanError::WorkerPoolExhausted { last_spawn_error }`. Retitle str-qwua7.49 accordingly.

### F11 [P2, L5] Invariant inference over-generalises

- `spec.rs:395-400` runs function-level invariant detection on every execution with no minimum sample. Per-class detection needs ≥2 samples (`:411`).
- `invariants.rs:504-535` accepts any template that holds on all specimens, so one or two specimens yield `x == <constant>` and `x > 0` "invariants".
- `ClassifiedInvariant.confidence` is hard-coded to 1.0 (`:659`), which makes the field meaningless.
- Templates are limited to 0-anchored comparisons and constants (`:253-340`).
- Recommendation: require a minimum support (Daikon-style, e.g. ≥5 specimens and a justification test), make `confidence` real or remove it, and add range (`lo <= x <= hi`) templates.

### F12 [P2, L4] Z3 has no default query timeout; the prior audit said otherwise

- `--solver-timeout`: "Default: no limit" (`shatter-cli/src/args.rs:658-660, 1074-1075`). `solve_for_new_path_impl` only sets `set_timeout_msec` when `Some` (`solver.rs:1214-1217`).
- With `solver_offload`, a pathological string or non-linear query occupies a blocking thread past `timeout_explore`, because Z3 is not cancelled.
- The prior audit's design-foundation table says "Timeouts: `cfg.set_timeout_msec` on each query", which is only true when the flag is passed.
- Recommendation: a default of e.g. 2 s, overridable, and log `SolverError::Unknown("timeout")` as a frontier stall.

### F13 [P3, L1] Float constants are converted by `(v * 1e6).round() as i64`

`solver.rs:514-516`: thresholds with |v| > ~9.2e12 saturate to `i64::MAX/1e6`, and |v| < 5e-7 round to 0, both silently. Recommendation: build exact rationals from the f64 bit pattern, or use `Real::from_real_str`.

### F14 [P2, L5] Explore auto-resume ignores option changes, including the explorer mode

- `try_resume_function` (`shatter-cli/src/commands/explore.rs:1209-1232`) keys only on the deep source fingerprint.
- Evidence: after a `--concolic` run, `shatter explore mix.go --max-iterations 40` (random) printed `[info] Resumed 2/2 function(s) from prior artifacts` and re-emitted the concolic results (16 Loopy paths) without the "Explorer: concolic" line.
- Changing `--max-iterations`, mocks or mode silently returns stale results. The only workaround is `--clean`.
- Partial resume (`read_resume_state`, `:5263`) can also load a concolic `covered_paths` set, keyed by `hash_branch_path`, into a run that uses the other hash space (F7).
- Recommendation: include a hash of the exploration-relevant options (mode, budgets, mocks, seeds, setup) in the summary entry and the resume-state sidecar, and log why a resume was rejected.

### F15 [P3, L6] "N/N branches" counts branch IDs, not branch sides

The progress lines report `branches_covered: discoveries.len()` (`orchestrator.rs:2847`), i.e. distinct branch IDs. `Classify` prints `2/2 branches` while one side (`x<1` true) was never taken and line coverage is 80%. Recommendation: report covered sides out of 2×branches, or label the metric "branch points seen".

### F16 [P3, L1] `core_sample` reproducibility uses `std::hash::DefaultHasher`

`core_sample.rs:389, 550` (`default_seed`, `stable_hash`). std documents that the algorithm may change between releases, and there is no `rust-toolchain.toml`. `--seed` promises that the same seed yields the same exploration (`args.rs:971-977`) and progressive `--batch next` relies on stable selection. Recommendation: use a fixed hash (e.g. `siphasher` with fixed keys, or FNV/xxhash).

### F17 [P3, L2] `shatter-core/CLAUDE.md` "Key Modules" describes the wrong engine

- `CLAUDE.md:7` says `explorer.rs — Concolic exploration loop`. It is the random/hybrid explorer; `orchestrator.rs` (described as "Multi-round exploration orchestration") is the concolic engine.
- It lists the dead `export.rs` and omits `solver.rs`, `strategy.rs` and `shrink.rs`.
- Subagents primed by this file will reason about the wrong parallel path, which is the drift hazard the root CLAUDE.md warns about.
- Recommendation: rewrite the module list around the two engines, the solver, strategy and shrink.

## 4. Agent-system root causes (cross-cutting)

1. **Parallel-path parity is enforced only by prose.** Root CLAUDE.md says to "grep for the parallel code path". No test runs both engines on the same fixture and compares observable outputs such as path counts, setup effects, mock variation and capture. F2, F3, F4, F7 and F9 are all engine-parity failures. Recommendation: add an `engine_parity` E2E suite (a table of fixtures × {random, concolic} with shared expectations) to `task e2e`.
2. **Tests call internal APIs, not the pipeline.** F3's test passes a context straight to `orchestrator::explore`. CLAUDE.md warns exactly against modules "silently disconnected from the pipeline", but the E2E harnesses keep calling mid-level APIs. Recommendation: in the E2E conventions, require at least one pipeline/CLI-level assertion per feature.
3. **`_`-prefixing silences regressions.** F4's `_mock_params` and `_initial_mocks`. Recommendation: rust-conventions rule that an unused-by-design parameter needs a `// TODO(str-xxx)` issue reference, with a lint script that flags `_`-prefixed params without one.
4. **Issues are closed on the evidence of the touched module, not the running path.** str-55ep (F5) and str-0s76.6 (F3) were both closed on unit or test evidence of a path production doesn't use. Recommendation: closure requires naming the production call site exercised (`grep` for callers) in the close reason. This belongs in the bento issue-flow and the pre-completion skill.
5. **No dead-code gate** (F8). `cargo-machete` covers dependencies only. Recommendation: a module-reachability check in `task check`.
6. **Prior audit graded dead or broken components as Solid** (clustering, solver timeouts). Audit skills should require a caller check and one behavioural probe per graded component.

## 5. Positives worth preserving

- The `str-jeen.65` wall-clock anchoring: every phase (float probe, main loop, refine, shrink) checks `deadline_crossed()`, and the stop reasons are precise.
- The frontend taint-on-timeout design (`frontend.rs:150-160`) avoids request/response ID desync, which is a hard problem to get right.
- `planner_consumer::execute_inputs_for_plan_with_pins` is a single funnel for input repair and native pins across engines. That is the right shape; the same should be done for building Execute requests.
- The enum-domain handling in the solver (`str-mambd`, enum history against oscillation) is well documented and tested.
- `WorkerTaskLease` and the dynamic watchdog in scan give careful accounting under abort.
- TODO hygiene is good (few TODOs, all ticketed), and module docs cover nearly every file.
