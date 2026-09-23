# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

**DEGRADED review.** The Codex counterpart failed identity validation (exit 4), so this is a same-runtime (Claude) fallback. It was read-only. Claims were checked against the worktree at `56c86168` (HEAD == `56c86168`, so line numbers match), and duplicates were checked against the live `bd` DB in /home/ketan/project/shatter.

## Summary

Nearly all code citations check out: solver.rs VarTable/to_z3_expr/extract_concrete_values/assert_int_param_ranges, the explorer float-probe `seen_paths` inserts, render.rs `unique_paths`, the orchestrator `setup_context=None` callers, the explorer teardown-before-shrink order, `_initial_mocks`/`_mock_params`, the live vs dead shrinkers and commit f40facf1, the refine Execute fields, the spec/invariants lines, the `--solver-timeout` help/solver/main.rs `solver_timeout: _` lines, and the scan_orchestrator pool code. The main problem is duplication. Two of the nine new issues are already filed as open bugs from the 2026-09-13 intake, and the bundle does not mention them.

## Findings

### BLOCKER: z3-mixed-int-real-sort-split duplicates open str-t854z
Open P1 bug str-t854z, "Numeric variables split across solver sorts" (created 2026-09-13), describes the same defect: VarTable's separate int/real maps, the hint override in to_z3_expr, and real overwriting int in extract_concrete_values. It also has its own direct repros, a mixed-literal property requirement and a TS concolic fixture requirement. Filing this draft would create a duplicate P1. Convert it to a note on str-t854z that adds the new evidence (the u8 range-bound twin, the Go `Classify` E2E, and the random engine also missing the branch) and the Go E2E acceptance item.

### BLOCKER: float-constant-rational-conversion duplicates open str-aureo
Open P2 bug str-aureo, "Float constants lose precision before solving" (created 2026-09-13), cites the same `solver.rs:514-516` `(*v * 1_000_000.0).round() as i64` encoding. The draft's claim "No existing issue covers them" is false. Convert it to a note on str-aureo that adds the saturation above ~9.2e12 and the zero-collapse below 5e-7 edge cases, if str-aureo lacks them. The draft's P3 also conflicts with str-aureo's P2.

### MAJOR: concolic-refine-execute-builder has an unachievable grep acceptance criterion
The AC says to migrate the sites "in both `orchestrator.rs` and `explorer.rs`" and then requires that a grep for `Command::Execute {` outside the helper and tests returns nothing. Inline Execute construction also exists in `genetic_explorer.rs` (1), `observe.rs` (1), `revalidation.rs` (1) and `recursive.rs` (4), plus the `frontend.rs`/`protocol.rs` definitions. So the AC either quietly widens scope to every engine or cannot pass. Pick one: list every in-scope file, or restrict the grep to the two engine files. The issue also bundles two separable fixes, the builder refactor and counting refine-phase paths. Consider splitting.

### MAJOR: the bundle summary and dependency notes are wrong once the duplicates are removed
The header counts (9 new: P1 x3, P2 x5, P3 x1) and the cross-references ("Related: z3-mixed-int-real-sort-split", the "coordinate" note in float-constant-rational-conversion, and the D3 note linking z3-mixed-int-real-sort-split to concolic-early-termination) all point at slugs that should become str-t854z / str-aureo. The filer script would otherwise leave dangling slug references.

### MINOR: invariant-min-support overstates "the spec cannot signal weak support"
`ClassifiedInvariant` carries `satisfied_count`/`total_count`, and the markdown spec renders `(n/n)` (`spec.rs:572-575`). Only the YAML output hides them (the test at `spec.rs:2767-2775`). Also, `invariants.rs:332` is the `NumericConstant` placeholder, not a fourth `NumericComparison` against 0. The 0-anchored comparison templates are at :267/:288/:309. Correct the evidence text.

### MINOR: z3-default-query-timeout names a type that does not exist
The Suggested approach says "where `SolveResult::Unknown` is handled in the orchestrator". `SolveResult` has only `Sat`/`Unsat`. Z3 Unknown becomes `Err(SolverError::Unknown(..))` (`solver.rs:1413`), and nothing outside solver.rs matches on that variant. The "not silently treated as UNSAT" AC therefore has the wrong premise: today it is an error path, not UNSAT. Restate it in terms of how callers handle the `SolverError`. The issue also bundles three changes (default timeout, the scan flag wiring, a stall counter). That is acceptable but broad.

### MINOR: qwua7-49-rescope cites the checkout line one off
`pool.checkout().await` is at `scan_orchestrator.rs:4608`, not :4607 (which is a comment). The rest verifies: there is no `try_send`, the `send().await.expect` calls are at 2241-2254, and `checkout` is at 2268-2271.

### MINOR: float-probe-paths-uncounted fixture paths are underspecified
The AC names "TS safeDivide", "Rust `safe_divide`" and `fmt2` without file paths. `safe_divide(f64, f64)` exists only under `audits/2026-09-22/goals-runs/standalone/rust/04_errors.rs` and inline in `shatter-rust/src/executor.rs` tests, and `examples/` has no `ts/` dir. Give each fixture's location, or say which ones must be created.

### MINOR: concolic-setup-teardown: the explorer shrink-after-teardown bug applies only to per-function setup
The teardown at `explorer.rs:1677` is gated on `per_function_setup && !skip_setup`. The draft states this correctly, but the AC should name the setup level in the test so the test does not pass trivially under a global setup level.

## Drafts verified without substantive issues
float-probe-paths-uncounted (code mechanism confirmed), concolic-setup-teardown, setup-parity-reopen-note, concolic-mock-variation-regression, mock-variation-reopen-note, duplicate-value-shrinkers (f40facf1 touched only input_gen.rs, and the live shrink.rs removes all fields and emits -1), qwua7-49-rescope (premise refutation confirmed against the str-qwua7.49 body). No open duplicates were found for these.

## Verdict
**Not ready to file as-is.** Top fixes:
1. Convert z3-mixed-int-real-sort-split into a note on str-t854z, and float-constant-rational-conversion into a note on str-aureo. Update the bundle counts and all slug cross-references.
2. Fix the concolic-refine-execute-builder grep AC so its scope is explicit and achievable, and consider splitting refine-path accounting from the builder refactor.
3. Correct the minor factual errors (invariant support visibility, `SolveResult::Unknown`, checkout line) before filing.
