# Shatter — Design & Goal-Fulfillment Review (Foundation Deep-Read)

Reviewer scope: Q4 "Is the design good?" and Q5 "Does the project achieve its stated goals?"
Method: read-only source inspection of `shatter-core`, `shatter-cli`, docs, and pre-captured
CLI help/run outputs. No builds or tests run. All line refs are HEAD as of 2026-09-03.

## 0. Stated goals (quoted)

| Source | Goal text |
|---|---|
| README.md:3 | "Automatic exploratory testing via concolic execution." |
| README.md:5 | "Shatter analyzes functions, discovers branch behavior, and generates inputs that exercise distinct paths without hand-written test cases." |
| README.md:388-395 | "1. Analyze the target… 2. Execute with concrete inputs while collecting path information. 3. Generate new inputs to reach uncovered behavior. 4. Report observed behaviors, clusters, and optional specs or tests." |
| SPEC.md §1 | "uses **concolic execution** (concrete + symbolic) to discover execution paths through functions, generate inputs that exercise each path, and produce behavioral specifications and regression snapshots." |
| SPEC.md §1.2 | "**CLI** (`shatter-cli`): Thin clap wrapper." / Core "orchestrates analysis, exploration, clustering, spec generation, and snapshot diffing." |
| SPEC.md §1.3 | TypeScript, Go, Rust all "Supported". |
| PLAN.md:22-30 | v2 motivation: v1 "coverage-guided random fuzzing… fails for non-trivial functions… The fix is concolic execution". |
| docs/execution-adapters.md:54-65 | "Keep `shatter-core` framework-agnostic… Give future TypeScript, Go, Rust, and other frontends one consistent model." |

## 1. Component table

| Component | Rating | Evidence |
|---|---|---|
| Explorer loop (`explorer.rs`, 7,115 lines, `explore_function` @1012) | **Solid** (not random-only) | Default explorer runs a meta-strategy `[UserProvided, Literals, PoolSeeds, BoundarySeeds, Z3Solver, Random]` (`strategy.rs:189-227`). `Z3SolverStrategy::feedback()` "extracts symbolic constraints… negates each solvable constraint, solves with Z3, overlays solutions" (`strategy.rs:1174-1177`, calls `solve_for_new_path_with_enum_history` @1248). Adaptive scheduler weights strategies by discovery yield. So "random explorer" is a misnomer: it is a strategy-scheduled hybrid with Z3 as one reactive strategy. Empirical: `classifyNumber` → 4/4 paths, 100% lines in 20 iters (ui/ts-explore.out); Go `Categorize` → 2/2 in 5 iters. |
| Concolic orchestrator (`orchestrator.rs`, 7,524 lines, `explore_with_oracle` @2424) | **Solid, heavy** | Worklist = `BinaryHeap<WorklistEntry>` ordered by genetic fitness then `InputSource` (@243-300). Negation: per new observation, each solvable branch decision in the path is negated with its prefix (`SymConstraint::Expr` only; `Unknown` skipped @3256), MC/DC independence goals solved via `solve_for_mcdc_independence` (@3232-3300). Termination enum (@304-318): MaxIterations, MaxExecutions, CoveragePlateau, WorklistExhausted, TimeoutExplore, McdcComplete. Budgets: per-function wall clock anchored at start (@2580), per-query Z3 timeout capped by remaining budget (@2544), fuzz-phase budget clamped to global cap (`clamp_fuzz_budget` @334). Fallbacks: on plateau, opaque branches (`Unknown` constraints) enter a mutation fuzz phase with corpus (@2851-3040); drilling, boundary search, float probe, LLM oracle slot (priority 4). Loop handling: bounded unroll + loop-invariant detector (@827-1000, 1821). This is a genuine concolic engine, not a stub. |
| Solver (`solver.rs`, 3,930 lines) | **Partial (deliberately)** | Sorts: Int, Real, Bool, Str (`Sort` enum @58). `TypeInfo → Sort`: arrays/objects/unions default to Int (@154) — silently lossy. Binops: comparisons, +−×÷mod on Int and Real, And/Or supported; `BitwiseAnd/Or/Xor/Shl/Shr/BitClear/In/InstanceOf` → `Unsupported` (@601-612). Unops: Not/Neg/BitwiseNot/TypeOf. Floats are Z3 `Real` with rational scaling `×1e6` (@514-516) — no IEEE semantics (NaN/inf/rounding unmodeled). Strings: Z3 Seq with 8 ops from `string-ops.yaml`, raw `Z3_mk_seq_index` FFI (@855); split/regex explicitly deferred (@798-810). Arrays: no Z3 array/seq theory; object sub-paths handled by name mangling (`config.timeout`) with `overlay_solved_values` field-path resolution (orchestrator @1176-1510). Model extraction (@1023-1070) covers 4 sorts; enum domains asserted (@188-395). Timeouts: `cfg.set_timeout_msec` on each query (@1215,1255,1344). Contracts on FFI boundary per CLAUDE.md. Good engineering; theory coverage is the ceiling. |
| Behavior maps / clustering (`behavior.rs` 2,225; `equivalence.rs`; `clustering.rs`) | **Solid** | `group_into_classes` (equivalence.rs:111) keyed on `BranchPath(Vec<BranchStep>)`; canonical example = simplest JSON; `cluster_by_branch_path` (clustering.rs:157) adds `ValueRange`/`ClusterStats`. `BehaviorMap` carries `DependencyTrace` for mock derivation (`mock_gen::mock_config_from_behavior_map`) and snapshot diff. Completeness for test gen: one representative input+output per path — sufficient for example-based tests, not for property tests. |
| Invariant detection (`invariants.rs`, 1,404 lines) | **Partial** | 9 Daikon-style kinds (SPEC §3.4 table matches `InvariantKind` @35). Soundness: purely observational; `detect_invariants` only returns invariants that hold for all specimens, so `confidence` is hard-coded `1.0` (@661 comment: "detect_invariants only returns invariants that hold for all specimens"). SPEC's "confidence scores (satisfied_count / total_count)" is therefore vacuous today — the score never varies. No minimum-sample threshold seen; with 2 samples `x > 0` will be "detected". No implication/disjunction, no cross-param relations beyond `OutputEqualsInput`. |
| Test generation/export (`export.rs`, 1,743 lines) | **Partial** | Generators: `generate_jest_tests`, `generate_vitest_tests`, `generate_go_tests` (+MC/DC-annotated variants). **No Rust test generator** despite Rust being "Supported" in SPEC §1.3. Output is example-based `expect(fn(args)).toEqual(out)` / table-driven `t.Run`; errors are asserted via throw-matching. Unit tests check string content, not compile/run — runnability of emitted Go (import paths, struct literals) is not verified in-crate. |
| Input generation (`input_gen.rs`, **9,010 lines**; `boundary_dict.rs`) | **Solid but a god module** | Type-directed random (`generate_random_value` @122), `repair_required_fields`, biased floats, mutation/havoc/crossover/shrink, custom generator protocol (`ValueSource`, prefetch, `NativePins` for opaque handles), pool seeding, literal extraction. Boundary dictionary categorized (Overflow/etc.) with i64::MAX, f64::MIN_POSITIVE, π/e etc. (boundary_dict.rs:119-259). Solver respect: solved values overlaid on base inputs and type-checked (`overlay_solved_values`, `solved_values_match_param_types` contract). 9k lines in one file mixing mock-value generation (`generate_mock_values` @4103) with input mutation is the maintainability problem. |
| Protocol / frontend abstraction (`protocol.rs` 4,154; `frontend.rs` 1,328) | **Solid boundary, leaky edges** | `Frontend::spawn` runs one long-lived NDJSON child (`frontend.rs:169-196`, `kill_on_drop`), with graceful shutdown/force-kill (@518-603) and stderr capture. Protocol is versioned (`PROTOCOL_VERSION`) with a YAML registry. Leakage: 42 non-test `Language::Rust` match sites across 9 files; `config.rs` has 70 language-token hits, `export.rs` 57, `test_runner.rs` 25, `scope.rs` 22, `crypto_registry.rs` 22. Core is language-*aware* by design (it emits Jest/Go tests), but it is not registry-driven — see §2.5. Adapter model in core (`adapter_selection.rs`) is generic as the doc demands: adapter ids are opaque strings, tests reference `ts/react-hooks` only as data. |
| Execution adapters / modes | **Partial, accreting** | Core: `IsolationMode { None, Function, Serial }` (explorer.rs:88). TS frontend: direct call, worker (`worker.ts`, `instrumentation-worker.ts`), `react-hook-invocation.ts` + `react-shim.ts` + `react-hook-recognizer.ts`, `browser-dom-adapter.ts` + `browser-globals-recognizer.ts`, `fs-write-redirect.ts`. Go: `harness/`, `sandbox/`, `overlay/`, `launcher/`, `wrapper/`, `workspace/` packages. Rust: harness-backed + Tokio/Axum adapter. That is ≥3 distinct execution substrates per language plus a sandbox backend switch (`SHATTER_SANDBOX_BACKEND=docker|bwrap`) and host-write gating. Coherent *intent* (docs/execution-adapters.md is a good design doc) but the doc itself says the React shim "should be treated as migration material, not the target architecture" (@738-751), i.e. the registry/recognizer substrate the doc calls for is only partly landed. |

## 2. Design assessment

### 2.1 Module dependency graph — P1

Fan-in (`use crate::X` count): protocol 46, types 30, execution_record 25, orchestrator 13,
behavior 13, sym_expr 12, explorer 11, coverage_metrics 11, frontend 10. Fan-out leaders:
orchestrator 19, scan_orchestrator 15, pipeline 10, genetic_explorer 10, explorer 10.

Confirmed mutual-import cycles (both directions of `use crate::`):
- `explorer <-> orchestrator` (explorer imports `orchestrator::{FrontendCapabilities, hash_branch_path}`; orchestrator imports `explorer::{apply_live_first_overrides, update_live_first_states, ProgressHints}`)
- `explorer <-> scan_orchestrator` (explorer.rs:175-178 embeds `scan_orchestrator::{BudgetSurplus, ClaimPolicy}` in `ExploreConfig`)
- `orchestrator <-> strategy`
- `input_gen <-> orchestrator`

Rust tolerates intra-crate cycles, but they show layering has collapsed: a leaf config type
(`ExploreConfig`) depends on the top-level scan scheduler. `hash_branch_path` and
`FrontendCapabilities` are leaf utilities living in a 7.5k-line orchestrator. No cycles
involve `protocol`/`types`/`sym_expr`/`solver` — the true leaves are clean.

God modules (lines): `scan_orchestrator.rs` **15,250**, `input_gen.rs` 9,010,
`orchestrator.rs` 7,524, `explorer.rs` 7,115, `report.rs` 6,430, `config.rs` 4,286,
`protocol.rs` 4,154, `solver.rs` 3,930. Eight files > 3.9k lines in a 97-module crate
(~150k lines total). `lib.rs` is a flat list of 96 `pub mod` — no layering expressed.

Also: `shatter-core` has `shatter-llm` as a **dev-dependency** while `shatter-llm` depends on
`shatter-core` (shatter-core/Cargo.toml:44; shatter-llm/Cargo.toml:9). Cargo allows this
dev-dep cycle but it forces `shatter-llm` to build before core's tests and couples the
oracle plugin to the engine. P3.

### 2.2 Random explorer vs concolic orchestrator — P1 (design debt, not unsound)

Two full exploration engines exist behind one flag:
- `explorer::explore_function` (default): meta-strategy scheduler with Z3 as a reactive strategy.
- `orchestrator::explore_with_oracle` (`--concolic`): worklist + explicit path negation + MC/DC + fuzz phase + drilling + oracle.

`shatter-cli/src/main.rs` threads a bare `concolic: bool` through every command
(lines 189, 354, 423, 478, 725, 803, 818) and `args.rs` documents it 4 times with three
different wordings (@449, 722, 1043, 1223). Both engines share solver, input_gen, coverage,
and strategy code but duplicate: termination classification (explorer.rs:1996 comment
literally says it mirrors `orchestrator::TerminationReason` "which cannot itself be" reused),
plateau detection, budget anchoring, and progress reporting. SPEC §3.5 says Z3 is
"Available in the default explorer and driven end-to-end by the concolic explorer" — the
user cannot tell from that which one to pick, and the README never mentions `--concolic`.

Is it sound? Yes — both produce valid paths. Is it good design? No: the flag is a hidden
fork of the core algorithm, doubling test surface, and the "random" engine already has the
concolic loop inside a strategy. Unification strategy:
1. Make `Z3SolverStrategy` the *only* negation site by moving orchestrator's
   prefix-aware negation + MC/DC goals into it (they are already pure functions of
   `(constraints, index, param_infos)`).
2. Express orchestrator-only phases (fuzz-on-plateau, drilling, float-probe, oracle) as
   additional `InputStrategy` impls with `feedback()`; the scheduler already has priority tiers.
3. Collapse `TerminationReason`/`StopReason` into one enum in a leaf module.
4. Keep `--concolic` as a *preset* of strategy weights for one release, then delete.

### 2.3 Config surface — P1

Flag counts from captured `--help` (approx, counting `-x/--xxx` lines): explore **79**,
scan **69**, run 30, properties 23, bench 22, observe 22, stale 22, test 21, revalidate 20,
specify 19, list-targets 18, analyze 17, discover-deps 16, solve 15, build-frontend 15,
top-level 14, plus 13-14 for cache/compare/diff/doctor/init/nondeterminism/spec-diff/
telemetry/workspace. 24 subcommands, ~600 flag instances. `args.rs` is 4,123 lines and
carries a comment that the `Cli` enum "lives close to the clap-derive stack budget" and has
overflowed a 2 MB thread stack before (@998-1006); `Explore`/`Scan` are already `Box`ed and
`LlmOverrides` is boxed for the same reason.

Config model: documented precedence is `CLI > --set > .shatter/config.yaml (nearest) >
shatter.config.json > defaults` (README:206, config.rs:472). That is coherent on paper.
In practice there are **four** channels: flags, `--set KEY=VALUE`, two YAML/JSON files with
disjoint schemas, and ~25 `SHATTER_*` env vars (SHATTER_CACHE_DIR, SHATTER_SEEDS_DIR,
SHATTER_HARNESS_RELEASE are clap `env=` so they sit *between* flag and file; others such as
SHATTER_SETUP_TIMEOUT/SHATTER_EXEC_TIMEOUT/SHATTER_SANDBOX_BACKEND/SHATTER_PATH_FEEDBACK_MODE
are read ad hoc and are not in the precedence table). Explore and Scan duplicate ~50 flags
by copy (args.rs:394/837, 422/866, 529/921 are literal pairs). The explore→scan→run
progression means the same knob is exposed three times.

Recommendation: a single `ExploreOptions` struct with `#[command(flatten)]` shared by
explore/scan/run/observe; move rarely-used tuning (strategy weights, score window, cold
start, genetic population, loop buckets, telemetry) to config-file-only with `--set`; make
every `SHATTER_*` var either a clap `env=` or documented in one table.

### 2.4 `shatter-core` public API — P2

`lib.rs` exports 96 modules `pub mod`, every one public, with no facade, no `prelude`, no
`pub(crate)` layering, and no doc on what is stable. Effective API for a consumer is every
`pub fn` in ~150k lines. Since the only consumers are `shatter-cli` and `shatter-llm`, this
is tolerable, but it means the CLI (`helpers.rs` 2,478 lines, `commands/` 23 files) is not
the "thin clap wrapper" SPEC §1.2 claims — much orchestration glue (frontend availability,
language dispatch, embedded bundle extraction) lives in the CLI.

### 2.5 Extensibility: 4th language — P2

Hand-wired, not registry-driven. Checklist derived from the code (no such checklist exists
in docs):
1. `discovery::Language` enum + `from_extension` (discovery.rs:14-26).
2. 42 `Language::Rust` match sites across 9 files; `shatter-cli/src/helpers.rs` alone has 15
   (frontend availability check @503-584, autodetect caps @189, discovery↔CLI language
   mapping @831).
3. `export.rs` test generator (none exists for Rust — so at minimum the pattern is uneven).
4. `config.rs` (70 language-token hits: setup-file extensions, harness flags),
   `scope.rs`, `source_bucket.rs`, `test_runner.rs`, `crypto_registry.rs`,
   `target_manifest.rs`, `project.rs`.
5. `shatter-cli` embedded-frontend plumbing (`embedded_frontend.rs`, `embedded_go_frontend.rs`
   are separate hand-written files; Rust ships as a sibling binary — three different
   distribution mechanisms for three languages).
6. `protocol/registry.yaml`, `protocol/parity-matrix.yaml`, PARITY.md.
7. `data/string-ops.yaml` method-name mapping for the solver.

The execution-adapter doc gets this right in principle ("adapter registry", "recognizer
registry" per frontend), but core-side language dispatch has no equivalent `LanguageProfile`
trait/table. Adding Java (PLAN.md's original milestone) would touch ≥15 core/CLI files.

### 2.6 Performance / scalability — P2

Good: one long-lived frontend process per language session (frontend.rs:169; observed
"Spawned 1 frontend session(s)… 16 parallel worker(s)"), so it is **not**
process-per-execution. Analysis cache (`analysis_cache.rs`), behavior-map cache
(`cache.rs`, 2,856 lines), harness build cache, seed pool, and `prepare` caching for Rust.
Per-query Z3 timeouts and budget-anchored phases. Perf CI exists (`perf-ci.yml` runs
`perf/stable-scenarios.txt`).

Concerns: `IsolationMode::None` is the default and "assumes functions are side-effect-safe
and stateless" (explorer.rs:90-93) — shared-process state leakage between executions can
produce nondeterministic paths (there is a whole `nondeterminism.rs` 1,765 lines to triage
this after the fact, which is treating the symptom). Z3 is called synchronously inside the
async loop (`z3_solve_step`, orchestrator @1989) — the comment notes the fuzz phase runs
"inline"; a long Z3 query blocks the observer pool. `scan_orchestrator.rs` (15k lines)
holds scheduling, budget surplus, claim policy, mock derivation, and status export in one
file, which is the scaling bottleneck for maintenance rather than runtime. Observed
latencies for trivial functions (TS 4.4 s, Go 2.5 s for 5-20 iterations) are dominated by
frontend spawn/instrumentation, not solving.

### 2.7 Accreted subsystems — P2

| Dir | Last commit | Commits | In workspace / CI? | Verdict |
|---|---|---|---|---|
| `shatter-llm` | 2026-07-07 | 11 | Yes (workspace member; ci.yml runs clippy+tests; dev-dep of core) | **Live but peripheral**: oracle plugin (`reqwest`), `SeedOracle` trait in core `oracle.rs`. Cycle with core via dev-dep. |
| `shatter-rust-runtime` | 2026-08-28 | 18 | Excluded from workspace; own Taskfile; cached in ci.yml | **Live** (Rust harness runtime). |
| `perf/` | 2026-08-26 | 13 | `perf-ci.yml` | **Live**. |
| `shatter-vs` | 2026-04-05 | 220 | Not in Taskfile or any workflow; not referenced in README/docs/INDEX/CONTRIBUTING | **Dormant v1 residue**: `src/core/{generator,hybridize,seed,shatterproof}.ts` are the v1 "hybridize/breed inputs" fuzzer PLAN.md says v2 replaced; VS Code extension shell, version 0.0.1, empty description. 5 months stale. |
| `benchmarks/` | 2026-03-27 | 4 | Referenced by perf-ci.yml as optional baseline dir only | **Half-built**: `baselines/` + one sample manifest. |
| `shatter-report/` | 2026-03-03 | 2 | No | **Stale artifact** (one scan-report.json/.md checked in). |
| `cross/` | 2026-02-25 | 1 | Cross.toml at root | **Stub**: single aarch64 Dockerfile. |
| `shatter-go-tool/` | 2026-05-14 | 1 | Documented in docs/distribution.md | **Thin wrapper**, intentional. |

Three of eight (`shatter-vs`, `shatter-report`, `cross`) are dead weight that should be
deleted or moved to a `contrib/`/archive; `benchmarks/` should be merged into `perf/`.

### 2.8 Other design notes

- **P2 Docs vs. reality drift is managed but real**: PLAN.md is honestly labeled historical;
  SPEC §7 has a "historical note" retracting its own earlier limitations. But SPEC §3.4's
  confidence-score claim is not what the code does, and SPEC §1.3's "Supported" for Rust
  hides the absence of a Rust test exporter and that the Rust frontend is not embedded
  (captured `rust-explore.err`: `STATUS skipped_by_unavailable_frontend`).
- **P3 Contracts/PBT policy** in shatter-core/CLAUDE.md is unusually well-reasoned (three-
  criteria test for contracts) — a genuine strength.
- **P3 `IsolationMode` lives in explorer.rs** but is a protocol/frontend concern.

## 3. Goal grades

| Goal | Grade | Justification |
|---|---|---|
| "Automatic exploratory testing via concolic execution" (README:3, SPEC §1) | **B+** | Two real concolic engines, Z3 negation with prefix constraints, MC/DC, loop unrolling, enum domains. Ceiling is solver theory: no bitwise, no arrays/objects beyond field-path mangling, floats as rationals, 8 string ops. Non-solvable branches fall back to fuzzing — the v1 mechanism PLAN.md set out to replace, now correctly positioned as fallback. |
| "discovers branch behavior, and generates inputs that exercise distinct paths without hand-written test cases" (README:5) | **A−** | Works end-to-end on captured TS and Go runs (100% path/line coverage, 0 config). Path-keyed equivalence classes with canonical examples. |
| "produce behavioral specifications and regression snapshots" (SPEC §1) | **B** | Spec (md/json/yaml), `proven`/`observed` provenance, snapshot + `diff`/`spec-diff` (captured specdiff.json shows working). Invariants are observational with a constant 1.0 confidence — the spec overstates. |
| "Report… optional specs or tests" (README:395) | **C+** | Jest/Vitest/Go emitters exist; no Rust emitter; runnability untested in-crate; example-based only. |
| TS/Go/Rust "Supported" (SPEC §1.3) | **B−** | TS and Go: embedded, verified working. Rust: separate sibling binary, tracked parity gaps (SPEC §7.1), no test export, not present in the captured environment. "Supported with caveats" would be accurate. |
| CLI is a "thin clap wrapper" (SPEC §1.2) | **D** | 10.9k lines in shatter-cli incl. 4.1k args.rs near clap's stack budget and 2.5k helpers.rs of language dispatch. |
| Core "framework-agnostic… one consistent model" for adapters (execution-adapters.md) | **B−** | Core side is honestly generic (`adapter_selection.rs`, opaque ids). Frontend side still has the ad hoc React shim the doc calls "migration material"; recognizer registries exist for TS only. |
| PLAN.md v2 thesis: replace random fuzzing with constraint-driven exploration | **B+** | Achieved; but the v1 code (`shatter-vs/src/core`) is still in-tree. |

**Overall: B.** The engine is real, the foundation modules (protocol, types, sym_expr,
solver) are clean and contract-guarded, and the product works end-to-end for TS and Go.
The design debt is concentrated and nameable: two parallel exploration engines, four
cyclic god modules, a 600-flag CLI at clap's structural limit, hand-wired language
dispatch, and ~3 dead subdirectories. None of it is architectural rot that requires a
rewrite; all of it is refactor-shaped.

## 4. Issue-ready recommendations

P1
1. **Unify explorers**: fold orchestrator negation/MC-DC/fuzz/drilling into `InputStrategy`
   impls; make `--concolic` a weight preset; delete duplicate termination/plateau/budget
   code. Acceptance: one `explore()` entry, one `TerminationReason`, `concolic: bool`
   removed from `main.rs` plumbing.
2. **Break the four core cycles**: move `hash_branch_path`, `FrontendCapabilities`,
   `BudgetSurplus`, `ClaimPolicy`, `TerminationReason` to leaf modules
   (`exploration_types.rs` / `budget.rs`); forbid `explorer ↔ scan_orchestrator` imports
   with a `cargo-modules`/`cargo deny`-style check in CI.
3. **Consolidate CLI args**: shared `#[command(flatten)] ExploreOptions` for
   explore/scan/run/observe; demote tuning flags to config/`--set`; single documented table
   for all `SHATTER_*` env vars in the precedence chain. Acceptance: args.rs < 2k lines,
   explore ≤ 40 flags, no `Box<…Args>` needed for stack.

P2
4. **Split `scan_orchestrator.rs`** (15k) into scheduler / budget / mock-derivation /
   status-export; split `input_gen.rs` (mock value generation → `mock_value_space.rs`
   which already exists).
5. **Language registry**: introduce a `LanguageProfile` table (extension, frontend
   binary/embedding, setup-file ext, test emitter, string-op table) and replace the 42
   `Language::Rust` match sites; write `docs/adding-a-language.md`.
6. **Add Rust test emitter** to `export.rs` or downgrade Rust to "Partial" in SPEC §1.3;
   add compile-check tests for emitted Jest/Go/Rust.
7. **Invariant confidence**: either implement `satisfied/total` with a min-sample
   threshold or remove the claim from SPEC §3.4. Add minimum-samples guard.
8. **Prune dead dirs**: delete or archive `shatter-vs`, `shatter-report`, `cross/`; merge
   `benchmarks/` into `perf/`. Document `shatter-llm` as optional plugin and remove core's
   dev-dep cycle (move the tests needing it into `shatter-llm/tests`).
9. **Land the frontend adapter registry** the execution-adapters doc specifies; migrate
   `react-shim.ts` behind `ts/react-hooks`; add Go/Rust recognizer registries even if empty.

P3
10. Move `IsolationMode` out of `explorer.rs` into `protocol.rs`; consider making
    `IsolationMode::Function` the default when nondeterminism triage flags state leakage.
11. Run Z3 queries on a blocking thread (`spawn_blocking`) so long solves don't stall the
    observer pool.
12. Solver: add BitVec sort for bitwise ops and integer-typed params (Go/Rust `u32`
    wraparound is currently unmodeled); add `Z3 Float` sort behind a flag for `f64`
    branches involving NaN/inf.
