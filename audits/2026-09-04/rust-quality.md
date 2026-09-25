# Rust Code Quality Review — Shatter workspace

Scope: `shatter-core`, `shatter-cli`, `shatter-llm`, `shatter-rust`, `shatter-rust-runtime` (read-only; no cargo build/test run).
Note: `shatter-report` does not exist as a crate in this checkout (no `shatter-report/Cargo.toml`).
Workspace members are `shatter-core`, `shatter-cli`, `shatter-llm`; `shatter-rust` and `shatter-rust-runtime` are excluded (separate builds).

Method: `wc`/`grep`/small Python scripts. "Non-test" = lines before the first `#[cfg(test)] mod …` pair (inline `#[cfg(test)]` on single items ignored). Counts are approximate but were spot-checked.

---

## 1. Size and structure

### Totals (src only)
| Crate | Lines | Files |
|---|---|---|
| shatter-core | 145,656 | 97 |
| shatter-cli | 33,854 | 33 |
| shatter-rust | 29,339 | 15 |
| shatter-llm | 2,600 | 11 |
| shatter-rust-runtime | 1,075 | 1 |
| **Total** | **~212k** (228k incl. `tests/`) | |

### 15 largest `.rs` files
| Lines | File | Non-test / test split |
|---|---|---|
| 16,125 | shatter-rust/src/executor.rs | — |
| 15,250 | shatter-core/src/scan_orchestrator.rs | 6,620 / 8,630 |
| 11,329 | shatter-cli/src/commands/explore.rs | ~4,116→6,800 non-test |
| 9,010 | shatter-core/src/input_gen.rs | 4,343 / 4,667 |
| 7,524 | shatter-core/src/orchestrator.rs | 3,732 / 3,792 |
| 7,115 | shatter-core/src/explorer.rs | 3,300 / 3,815 |
| 6,430 | shatter-core/src/report.rs | 2,807 / 3,623 |
| 4,475 | shatter-rust/src/analyzer.rs | — |
| 4,286 | shatter-core/src/config.rs | — |
| 4,154 | shatter-core/src/protocol.rs | — |
| 4,123 | shatter-cli/src/args.rs | — |
| 3,930 | shatter-core/src/solver.rs | — |
| 3,687 | shatter-cli/src/commands/run.rs | — |
| 3,041 | shatter-core/src/spec.rs | — |
| 2,913 | shatter-core/src/pipeline.rs | — |

- **[P1] 20 files > 2,000 lines; 87 files > 500 lines** (of 97 core files, only ~10 are under 500 lines). Files are single flat modules — `shatter-core/src/` has no subdirectories; every module is one file. The five biggest core files each carry more test code than production code in the *same* file, which is why they reach 7k–15k lines.
- **[P1] `shatter-cli/src/commands/explore.rs` (11,329 lines) contains `run_explore` at line 4116 which is 2,658 lines long with brace depth 15.** This is the single largest function in the workspace.
- **[P2] `shatter-cli/src/main.rs` is a partial monolith.** `async fn main()` (line 55) is 1,234 lines. `shatter-cli/CLAUDE.md` says main.rs "parses args and dispatches each CliCommand variant to a handler" — in practice the `Scan` arm is 300 lines (main.rs:467–767), `Explore` 231 lines (168–399), `ListTargets` 224 lines (1235–1296+). These arms destructure 40+ fields and do resolution/wiring logic that belongs in `commands/*.rs`. Smaller arms (`Init`, `Run`, `Diff`) are properly thin.
- **[P2] `shatter-rust/src/executor.rs` at 16,125 lines is the largest file in the repo** and is excluded from workspace clippy (`shatter-rust` is `exclude`d in the root Cargo.toml; lint via `rust-fe:clippy` task only).

## 2. Doc coverage

- **[P3 – good]** All 8 largest core files have `//!` module docs (3 lines each). Only 3 core files lack `//!`: `lib.rs` (has a 1-line `//!`), `report_style.rs`, `timing.rs`.
- **[P3 – good]** `///` coverage on non-test `pub` items is high: input_gen 51/51, orchestrator 17/17, explorer 19/19, report 42/42, protocol 45/45, frontend 14/14 documented. Gaps: `scan_orchestrator.rs:3805 pub async fn parallel_scan_with_progress` (the 1,346-line entry point) has no doc; `scan_orchestrator.rs:447 as_str`, `:468 ProgressHandler`; `config.rs:25-27` `DEFAULT_SCAN_TIMEOUT_*` consts; `solver.rs:1161 pub fn solve_for_new_path` (has contracts but no `///`).
- **[P2]** rust-conventions says `missing_docs` / rustdoc lints are "the enforcement path" — but no crate sets `#![warn(missing_docs)]` (`grep` finds it only inside a string literal at `shatter-cli/src/commands/explore.rs:10231`). Doc coverage is good by discipline, not enforcement.

## 3. `unwrap()` / `expect()` / `panic!` / `unreachable!` in non-test code

| Crate | unwrap | expect | panic! | unreachable! |
|---|---|---|---|---|
| shatter-core | 10 | 35 | 1 | 13 |
| shatter-cli | 52 | 8 | 0 | 3 |

rust-conventions: "Treat `clippy::unwrap_used` as the default policy for shatter-core: no `unwrap()` in library code." **[P2] `clippy::unwrap_used` is not enabled anywhere** (no `[lints.clippy]`, no `#![warn(clippy::unwrap_used)]`), so the 10 core unwraps are policy violations that nothing catches.

**Riskier sites (core):**
- `shatter-core/src/scan_orchestrator.rs:2270` `rx.recv().await.expect("pool should not be empty")` — mpsc `recv()` returns `None` when all senders drop, not only when empty; a worker-spawn failure path could panic the scan instead of erroring. **[P2]**
- `shatter-core/src/scan_orchestrator.rs:2244, 2254` `.expect("channel has capacity for …")` — `try_send` capacity assumption; panics if pool sizing drifts. **[P2]**
- `shatter-core/src/timing.rs:203, 207, 223` `self.inner.lock().unwrap()` — mutex poison → panic in a telemetry helper; use `unwrap_or_else(PoisonError::into_inner)`. **[P3]**
- `shatter-core/src/html_templates.rs:213, 273, 487` `tmpl.render().expect(...)` — askama render failure aborts the process from library code; should return `Result`. **[P3]**
- `shatter-core/src/types.rs:425` `panic!("positional_field_types: index {i} missing — caller must check …")` — documented precondition; a `Result`/`Option` return would remove the caller contract. **[P3]**
- `shatter-core/src/batch_scheduler.rs:333` `.expect("record_outcome called for a task_index that is not in flight")` — state-machine invariant on external caller. **[P3]**
- `shatter-core/src/recursive.rs:415, 456, 487` map `.get_mut(func_id).expect("func in map")` — cross-collection alignment (the exact class shatter-core/CLAUDE.md says should be a contract or type). **[P3]**
- `shatter-core/src/config.rs:1155` `path.last().unwrap()` — relies on caller passing non-empty path. **[P3]**
- `shatter-core/src/strategy.rs:959` `candidates.last().unwrap()` — guarded by earlier `total` check; safe but fragile. **[P3]**

**Clearly safe (no action):** `nondeterminism.rs:351-375` static `LazyLock<Regex>` (8 sites); `frontend.rs:203-205` after `Stdio::piped()`; `frontend.rs:644, 658` mutex with explicit poison message; `solver.rs:658-736` `unreachable!()` after exhaustive sort matches; `oracle.rs:168` Pending-after-`is_finished`; `behavior.rs:730-744` Tarjan stack; `orchestrator.rs:1454, 1469` after coercion; `explorer.rs:842` after empty-path early return; `scan_orchestrator.rs:3646, 3653` after `len()==1` / non-empty; `nondeterminism.rs:133` key from union of maps; `planner_consumer.rs:340`, `solver.rs:1389` "checked above".

**CLI:** 41 of 52 unwraps are `writeln!(md, …).unwrap()` into a `String` in `shatter-cli/src/commands/scan.rs:1954-2052` and `run.rs:1914-1982` — infallible (`fmt::Write` on String), but noisy; a `let _ =` or a small `push_line` helper would remove 41 unwraps. Remaining: `explore.rs:4549, 5449, 6242` `.expect("frontend/fe_config must exist for target language")` — depend on earlier map insertion; `embedded_frontend.rs:116` / `embedded_go_frontend.rs:120` UTF-8 file-name expects (safe for embedded paths). `args.rs:267`, `scan.rs:481`, `explore.rs:4629` `unreachable!` are guarded by clap conflicts / prior match. **[P3]**

## 4. Core files lacking `#[cfg(test)]`

Only `shatter-core/src/test_arbitraries.rs` (test-generator module; expected). Every other core module has a test block. **[P3 – good]**

## 5. Error handling

- **[P3 – good]** 30 `thiserror` enums in core (`solver.rs:19 SolverError`, `frontend.rs:38 FrontendError`, `config.rs:60`, `explorer.rs:541`, `orchestrator.rs:730`, `scan_orchestrator.rs:388`, etc.). `anyhow` appears in only one core file (`oracle.rs`, 7 uses) despite being a core dependency — **[P3]** the `anyhow = "1"` dep in `shatter-core/Cargo.toml` could be dropped if `oracle.rs` migrates.
- **[P3]** Stringly `Result<_, String>` survives in core public API: `stratum.rs:63 parse_stratum_spec`, `core_sample.rs:182 parse_sample_budget`, `:211 parse_batch_spec`, `mock_value_space.rs:130`, `fingerprint.rs`, `config.rs` (10 sites).
- **[P2]** shatter-cli uses **zero** `anyhow` (grep: 0 files) even though rust-conventions says "Use anyhow in shatter-cli entrypoints". Instead there are 56 `Result<_, String>` signatures (`commands/explore.rs` 19, `build_frontend.rs` 7, `args.rs` 7, `embedded_*_frontend.rs` 12). `main.rs:1335 error_exit_code` / `:1381 categorize_error` then classify errors by `Display` string matching. This is the pattern the convention was written to prevent.

## 6. Duplication and parallel-path drift

### explorer.rs (random) vs orchestrator.rs (concolic)
- **[P1] Two structs both named `ExploreConfig`** — `explorer.rs:101` (28 fields) and `orchestrator.rs:103` (19 fields). Only 11 fields overlap. Explorer-only: `budget_surplus candidate_inputs candidate_queue_capacity capabilities capture_side_effects claim_policy file isolation loop_buckets observer_frontend_config observer_pool pool_seeds prepare_id_override project_root setup_file setup_level user_seeds`. Orchestrator-only: `branch_profile fuzz max_executions mcdc plateau_threshold refine_budget solver_offload solver_timeout_ms`. The CLI translates one into the other by hand at `shatter-cli/src/commands/explore.rs:5138-5171`, which is exactly where new fields get forgotten.
- **[P1] Concrete drift: `capture_side_effects` is honored by the random explorer but hard-coded in the concolic orchestrator.** `explorer.rs:2784 capture: config.capture_side_effects` vs `orchestrator.rs:1630, 2981, 3514 capture: true` (and `capture: false` at 2314, 2632, 2645, 3558, 3612). The orchestrator's `ExploreConfig` has no `capture_side_effects` field at all, so `--capture-side-effects`/config cannot influence concolic execute requests. `side_effect` token count: explorer 51, orchestrator 3.
- **[P2] Token-level drift** (mentions in non-test code, explorer vs orchestrator): `fingerprint` 19 vs 0; `interesting_pool` 3 vs 0; `recursive` 4 vs 0; `entropy` 1 vs 0; `drilling` 0 vs 23; `symbolic_unroll` 0 vs 8; `boundary_search` 1 vs 11; `oracle` 2 vs 37. Some of this is by design (concolic-only solver features), but `fingerprint`/`interesting_pool`/`recursive` being random-only warrants a parity table.
- **[P1] Copy-pasted shrink pass:** `explorer.rs:1705-1760` and `orchestrator.rs:3434-3490` are a 36-line verbatim block (witness dedup → `should_shrink_path` filter → sort by complexity → `ShrinkStats`), followed by two more 10–15 line identical windows (`explorer.rs:1743`↔`orchestrator.rs:3478`, `explorer.rs:1906`↔`orchestrator.rs:3641`). 26 identical 8-line windows in total between the two files. Any bug fix to shrink selection must be made twice.
- `explore_function` (explorer.rs:1012, 965 lines) and `explore_with_oracle` (orchestrator.rs:2424, 1,307 lines) are the two parallel loops; neither shares a helper for execute-request construction (`Command::Execute { … }` is built inline at ≥8 sites across the two files).

### Other duplication
- **[P2]** `shatter-core/src/pipeline.rs:786-820` ↔ `scan_orchestrator.rs:1279-1313`: 34 identical lines.
- **[P2]** CLI: `commands/properties.rs:111-140` ↔ `commands/run.rs:540-569`: 29-line identical `ExploreConfig { … }` literal with all-default fields. `observe.rs:69-77`, `revalidate.rs:65-73`, `stale.rs:62-69`, `properties.rs:89` share the same analyze-request/response-unwrap block (14 identical windows properties↔run, 4 across observe/revalidate/stale). A `Default` impl or builder for `explorer::ExploreConfig` plus one `analyze_functions()` helper in `helpers.rs` would delete most of it.
- `shatter-core/src/call_graph.rs` ↔ `fingerprint.rs` (7 windows), `boundary_search.rs` ↔ `nondeterminism.rs` (5), `interesting_pool.rs` ↔ `protocol.rs` (5). **[P3]**
- Test-fixture literals (`ExecuteResult { lines_executed: vec![], calls_to_external: vec![], … }`) are repeated ~45 times across 13 core files instead of using `test_arbitraries.rs`. **[P3]**

## 7. Dependency direction

- `cli → core`: **[P3 – ok]** `shatter-cli` depends on `shatter-core` and `shatter-llm`; core never names `shatter_cli`.
- `shatter-llm → shatter-core` (fine), but **[P3] `shatter-core` has `shatter-llm` as a dev-dependency** (`shatter-core/Cargo.toml:44`, used only by `shatter-core/tests/e2e_llm_oracle.rs`). Cargo permits the dev-cycle, but it means core's test build depends on a crate that depends on core, and pulls `reqwest` into core's test graph. Consider moving `e2e_llm_oracle.rs` into `shatter-llm/tests/` or `shatter-cli/tests/`.
- `shatter-rust` (frontend) depends on no workspace crate — protocol types are re-declared locally (`shatter-rust/src/protocol.rs`, 1,865 lines), so "frontends → protocol" holds only by parity tooling, not by the type system. **[P3]** (known design, since it's excluded from the workspace.)

## 8. Clippy / lint configuration

- No `clippy.toml`; no `[lints.clippy]` tables. `shatter-core/Cargo.toml` has only `[lints.rust] unexpected_cfgs = { check-cfg = ['cfg(kani)'] }`. Gate: `cargo clippy --workspace -- -D warnings` (Taskfile.yml:178). No `#![allow]` at crate level except `test_arbitraries.rs:6 #![allow(dead_code)]` (fine).
- Item-level allows: **30× `#[allow(clippy::too_many_arguments)]`** (3 in scan_orchestrator.rs alone at :5644, :5768; target_manifest.rs:377), 1× `type_complexity` (scan_orchestrator.rs:3247), 5× `#[allow(dead_code)]` (`shatter-cli/src/commands/explore.rs:945, 956`, `helpers.rs:91, 133`). **[P2]** 30 `too_many_arguments` suppressions is a signal that parameter structs are overdue, consistent with the two hand-translated `ExploreConfig`s.
- Nothing suppresses safety-relevant lints, but nothing enables the ones the conventions call for (`unwrap_used`, `expect_used`, `missing_docs`, `rustdoc::*`). **[P2]**

## 9. Convention compliance — property tests

`proptest!` present in 53 of 97 core modules. **43 core modules lack any `proptest!` block.** The ones that most clearly violate "every non-trivial public function should have proptest coverage" (by size and semantic content):

| Module | Lines | Why it matters |
|---|---|---|
| behavior.rs | 2,225 | Tarjan SCC / behavior map — graph invariants |
| nondeterminism.rs | 1,765 | diff/classification of values; regex-based classifiers |
| executability.rs | 1,733 | opaque-type detection (policy: "trust boundary" on frontend output) |
| invariants.rs | 1,404 | invariant inference — ideal for PBT (inferred invariant must hold on the trace it was inferred from) |
| coverage_metrics.rs | 1,409 | monotonicity / bounds on coverage numbers |
| frontend.rs | 1,328 | subprocess JSON protocol boundary — policy tier 1 |
| discovery.rs | 1,290 | path globbing |
| call_graph.rs | 1,254 | topological layers / cycle detection |
| core_sample.rs | 1,140 | `parse_sample_budget`/`parse_batch_spec` parsers |
| snapshot.rs / spec_diff.rs | 1,042 / 1,156 | diffing — roundtrip & commutativity |
| types.rs / sym_expr.rs | 1,021 / 653 | central data types; `sym_expr` is the solver input |
| string_mutation.rs / array_mutation.rs | 618 / 397 | policy item 5: "all input generation (mutate…) must preserve type contracts" |
| equivalence.rs, clustering.rs, recursive.rs, execution_record.rs, float_probe.rs, oracle.rs, mock_gen.rs, scope.rs, stratum.rs, canonical_json.rs, checkpoint.rs, batch_state.rs, source_bucket.rs, run_manifest.rs | | |

Also: files whose only `proptest!` is a serialization roundtrip (policy calls this "table stakes") — not measured here but `explorer.rs`, `orchestrator.rs`, `report.rs` each have exactly one `proptest!` block for 7k+ lines. **[P2]**

Contracts: 4 `#[requires]`/`#[ensures]` in `solver.rs` (1158, 1176 + ensures) and 1 in `orchestrator.rs:1130` — matches the qualifying-sites table in shatter-core/CLAUDE.md. **[P3 – ok]**

## 10. Other smells

- **[P1] Giant functions (non-test, >500 lines): 8.** `run_explore` 2,658 (explore.rs:4116, depth 15); `run_scan` 1,393 (scan.rs:174); `parallel_scan_with_progress` 1,346 (scan_orchestrator.rs:3805); `explore_with_oracle` 1,307 (orchestrator.rs:2424, depth 11); `main` 1,234; `explore_function` 965 (explorer.rs:1012); `run_run` 590 (run.rs:148); `generate_jest_tests_with_annotations` 561 (export.rs:254, depth 12). 20 functions exceed 200 lines.
- **[P3] `clone()` density:** scan_orchestrator.rs 166 clones in 6,620 non-test lines; input_gen.rs 90 / 4,343; explorer.rs 73 / ~3,300. Many are `Vec<serde_json::Value>` / `Vec<MockConfig>` clones inside per-execution loops (e.g., explorer.rs:1707-1712 clones inputs+mocks twice per witness; orchestrator.rs same). Not measured for hot-path impact; worth a profile before refactoring.
- **[P3] TODO/FIXME:** only 4, all ticketed: `executability.rs:16 TODO(str-asnl)`, `pipeline_orchestrator.rs:530 TODO(str-qnp0)` (`function_source: String::new()` placeholder shipped), `commands/explore.rs:1695 TODO(str-hy9b.A2)` (stub `InvocationOutcome`). No FIXME/XXX/HACK. Good hygiene.
- **[P3] Magic literals in CLI wiring** despite the "named constants" rule: `explore.rs:5146 plateau_threshold: if mcdc { 60 } else { 20 }`, `:4327`/`:5149` `* 1000` sec→ms conversions.
- **[P3] `#[allow(dead_code)]` in cli** (`explore.rs:945, 956`, `helpers.rs:91, 133`) with comments "used by tests and the str-eam2 fallback path" — code retained for a fallback that may not exist.
- **[P3] `.cargo/config.toml` caps `jobs = 8` and `RUST_TEST_THREADS=4`** for all cargo invocations in the checkout (documented as agent resource governance); harmless but surprising for a solo dev.

---

## Recommendations (tracker-ready)

1. **Concolic orchestrator ignores `capture_side_effects`** — P1
   `orchestrator::ExploreConfig` has no `capture_side_effects` field and every `Command::Execute` in `orchestrator.rs` hard-codes `capture: true|false` (1630, 2981, 3514 vs 2314, 2632, 2645, 3558, 3612), while `explorer.rs:2784` honors `config.capture_side_effects`. Add the field, thread it from `shatter-cli/src/commands/explore.rs:5138`, and add an E2E case that runs `--concolic` with capture off and asserts no side-effect payloads. This is an instance of the CLAUDE.md "explorer vs orchestrator drift" hazard.

2. **Unify the two `ExploreConfig` structs behind a shared base** — P1
   `explorer::ExploreConfig` (28 fields) and `orchestrator::ExploreConfig` (19 fields) share 11 fields and are hand-translated in the CLI. Introduce a `CommonExploreConfig` (seed, timeouts, mocks, mock_params, execution_profile, meta_config, shrink_budget, planner, value_sources, default_execute_plan, capture_side_effects) embedded in both, and derive the orchestrator config `From<&explorer::ExploreConfig>` so new shared fields can't be forgotten. Rename one of them — two `pub struct ExploreConfig` in one crate is confusing at call sites.

3. **Extract the shrink-pass selection into `shrink.rs`** — P1
   `explorer.rs:1705-1760` and `orchestrator.rs:3434-3490` are a verbatim 36-line block (witness dedup by path hash, `should_shrink_path` filter, complexity sort, `ShrinkStats` init) plus two more identical 10–15 line windows. Move it to `shrink::select_witnesses(raw_results, config) -> (Vec<…>, ShrinkStats)` with a proptest for ordering determinism, and call it from both loops.

4. **Split `run_explore` (2,658 lines, depth 15)** — P1
   `shatter-cli/src/commands/explore.rs:4116` is the largest function in the workspace and the place where random/concolic/genetic wiring diverges. Extract at least: frontend/pool setup, per-function config resolution, the random-phase call, the concolic-phase call, and result finalization (some already exists as `finalize_explore`). Target: no function over 300 lines in `explore.rs`; move the `main.rs` `Explore`/`Scan`/`ListTargets` arms (231/300/224 lines) into their command modules so `main.rs` matches its own CLAUDE.md description.

5. **Enable the lints the conventions already claim** — P2
   Add `[lints.clippy] unwrap_used = "warn"` (core), `#![warn(missing_docs)]` + `rustdoc::broken_intra_doc_links` to `shatter-core/src/lib.rs`, and fix the ~10 core `unwrap()`s (timing.rs mutex locks, config.rs:1155, strategy.rs:959) and 41 `writeln!().unwrap()` in `scan.rs`/`run.rs`. Currently rust-conventions names these as "the enforcement path" but nothing enforces them.

6. **Adopt `anyhow` (or a `CliError`) in shatter-cli** — P2
   56 `Result<_, String>` signatures and `main.rs:1381 categorize_error` string-matching on `Display` output. Conventions say CLI entrypoints use `anyhow`; introduce it with `.context()` at the boundary and downcast to core `thiserror` types for exit-code classification instead of substring matching.

7. **Fix `rx.recv().await.expect("pool should not be empty")`** — P2
   `scan_orchestrator.rs:2270` panics when all pool senders drop (e.g., every worker failed to spawn), which is a reachable error path, not an invariant. Return `ScanError` instead; also review `:2244/:2254` `try_send(...).expect("channel has capacity")`.

8. **Property tests for the untested core modules** — P2
   43 of 97 core modules have no `proptest!`. Prioritize by policy tier: `frontend.rs` (protocol parse boundary), `string_mutation.rs`/`array_mutation.rs` (mutation contract: type + length preservation), `invariants.rs` (inferred invariant holds on its source trace), `behavior.rs`/`call_graph.rs` (SCC/topo-order invariants), `core_sample.rs`/`stratum.rs` parsers (roundtrip + malformed input). Reuse `test_arbitraries.rs`.

9. **Move `e2e_llm_oracle.rs` out of shatter-core** — P3
   `shatter-core` dev-depends on `shatter-llm`, which depends on `shatter-core`, creating a dev-cycle and pulling `reqwest` into core's test graph. Relocate the test to `shatter-llm/tests/` or `shatter-cli/tests/`, then drop the dev-dep; also drop `anyhow` from core once `oracle.rs` uses `thiserror`.

10. **De-duplicate CLI analyze/explore boilerplate** — P3
    `properties.rs:111-140` ↔ `run.rs:540-569` (identical 29-line `ExploreConfig` literal) and the analyze→`ResponseResult::Analyze` unwrap in `observe.rs:69`, `revalidate.rs:65`, `stale.rs:62`, `properties.rs:89`. Add `impl Default for explorer::ExploreConfig` (or a builder) and a `helpers::analyze_functions(frontend, file) -> Result<Vec<FunctionAnalysis>>`.

11. **Split the mega-files into module directories** — P3
    `scan_orchestrator.rs` (15,250), `input_gen.rs` (9,010), `orchestrator.rs`, `explorer.rs`, `report.rs` each carry more test lines than production lines in one file. Move tests to `<module>/tests.rs` (`#[cfg(test)] mod tests;`) and split production code into `<module>/{mod,config,loop,shrink,…}.rs`. No behavior change; unblocks review and reduces merge conflicts across swarm branches.
