# Revision: shatter-engine-correctness

Revised 2026-09-23 against the Codex cross-check (`crosscheck/shatter-engine-correctness.codex.md`, primary) and the earlier same-runtime review (`crosscheck/shatter-engine-correctness.md`, secondary). Nothing is filed (D6).

Checks done for this revision:

- Tracker state with `bd show`: str-t854z, str-aureo, str-qwua7.5, str-qwua7.47, str-qwua7.49.
- Code on the audit worktree. `git diff 56c86168 HEAD -- shatter-core shatter-cli` is empty, so the cited line numbers still hold.
- The pinned `z3-0.19.10` crate source, `Taskfile.yml` e2e targets, and the examples checkout at `/tmp/shatter-examples-main`.

## Codex findings

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| 1 | BLOCKER | 03/04: there is no `--setup` flag; setup comes from config | **Applied.** Verified: `args.rs` has only `--setup-timeout` (:699) and `--fail-on-setup-error` (:703), and `explore.rs:5051` reads `resolved.setup` from config. 03 and 04 are rewritten around a `.shatter/config.yaml` (`defaults.setup` / `setup_level`) reproduction. The CLI E2E is config-driven. | 03, 04 |
| 2 | MAJOR | 05: the allowed resolutions contradict the mandatory mock-variation test | **Applied.** Restoring the variation is now the only acceptable resolution; documenting and warning is removed. The E2E is pinned to `17-mock-branches.ts::classifyStatus` run through `orchestrator::explore`. New evidence: the existing `concolic_mock_*` tests actually run the random explorer, so the regression was hidden. They must be renamed. A seeded determinism test is added. The 06 note text is updated to match. | 05, 06 |
| 3 | MAJOR | 12: float corruption starts at 2147.483647, not 9.2e12 | **Applied.** Verified: `z3-0.19.10/src/ast/real.rs:63-75` casts `num as c_int`. 12 became a note on str-aureo with the wrap example (3000.0 becomes -1294.967296). The note proposes raising str-aureo from P2 to P1. | 12 (now `12-aureo-float-constant-note.md`) |
| 4 | MAJOR | 12: a shortest-round-trip decimal is not an exact rational | **Applied.** The round-trip option and round-trip assertion are removed. The note requires exact big-integer numerator/denominator equality against `f64::integer_decode`, with a fixed corpus (including 0.1) and a proptest. | 12 |
| 5 | MAJOR | 12: the required tests need model-extraction work that was declared out of scope | **Applied.** Extraction is now explicitly required (`as_rational` is `Z3_get_numeral_small`; the string-parse fallback fails on `(/ a b)` and drops the assignment). Translation tests and model round-trip tests are separated. str-aureo already has extraction in scope, and the note makes the failure mode concrete. | 12 |
| 6 | MAJOR | 01: the prescribed plain `cargo test --test e2e_*` skips the `#[ignore]`d tests | **Applied.** Verified 26/23/13 `#[ignore]` in the three suites, and `task e2e-*` runs `-- --include-ignored`. The note on str-t854z requires `task e2e-go` / `task e2e-ts` and quoting the new test names as run. The same rule is added to 02, 03, 05 and 12. | 01, 02, 03, 05, 12 |
| 7 | MAJOR | 01: fixture instructions ignore the external examples repo | **Applied.** The note names `github.com/shatterproof-ai/examples` via `SHATTER_EXAMPLES_DIR` / `scripts/examples_checkout.py`. It chooses self-contained inline tempdir fixtures (precedent: `TS_CLOSURE_FIXTURE`, `e2e_concolic.rs:277-279`), or else a pinned examples-repo change that lands first. 02 gets the same split, with named examples-repo paths. | 01, 02 |
| 8 | MAJOR | 03: per-execution setup semantics are undefined for the lifecycle helper and shrinking | **Applied.** Added a lifecycle table (function level: one live context through shrink, teardown after shrink; execution level: Setup/Teardown around each shrink attempt). Added the execution-level shrink gap to the Problem section, verified as no `send_setup` in the explorer shrink region. Shrink tests for each level assert the request sequence. | 03 |
| 9 | MAJOR | 08: bundles three deliverables and conflicts with str-qwua7.5's ownership and probe policy | **Applied (split).** Verified that str-qwua7.5 classifies the refine site as a discarded probe with `capture: false`. 08 is now only the `prepare_id`/`execution_profile` field fix. New 13 `concolic-refine-path-accounting` owns the policy change and must comment on str-qwua7.5. New 14 `execute-request-builder` is a behaviour-preserving refactor that takes `capture` as a parameter, so str-qwua7.5 keeps ownership of it. 08 no longer closes str-qwua7.5. | 08, 13 (new), 14 (new) |
| 10 | MAJOR | 11: the exhaustion predicate can reject a successful recovery | **Applied.** Verified that `maybe_grow` (:2336-2363) increments `live_count` before a detached spawn. The note now defines exhaustion as no live workers, no spawn in flight, and tasks waiting. It requires waking blocked checkouts and adds successful-retry and blocked-waiter regression tests. The checkout line is corrected to :4608. | 11 |
| 11 | MINOR | 09: optional range inference leaves "done" undefined; source-trace property overlaps str-qwua7.47 | **Applied.** Range templates moved to out of scope. The source-trace property is left to str-qwua7.47 (confirmed in its body). 09 now owns only minimum support, with a threshold proptest. The title is narrowed. | 09 |
| 12 | MINOR | 10: `SolveResult::Unknown` does not exist | **Applied.** Points to `SolverError::Unknown` (solver.rs:1413), the `Ok(Unsat) \| Err(_)` arm at orchestrator.rs:2110, and the strategy `_ =>` arm at strategy.rs:1268. The AC is rewritten as a separate counter that does not touch unsat accounting. | 10 |

## Secondary (same-runtime) findings

| Sev | Finding | Action | Files |
|---|---|---|---|
| BLOCKER | z3-mixed-int-real-sort-split duplicates open str-t854z | **Applied.** Confirmed with `bd show str-t854z`. Converted to note-to-existing `t854z-sort-split-note`. | 01 |
| BLOCKER | float-constant-rational-conversion duplicates open str-aureo | **Applied.** Confirmed with `bd show str-aureo`. Converted to note-to-existing `aureo-float-constant-note`. | 12 |
| MAJOR | 08: the grep AC cannot pass because of Execute sites in genetic_explorer/observe/revalidation/recursive | **Applied.** Counts re-verified. In 14 the grep is limited to orchestrator.rs + explorer.rs, and the other files are listed as out of scope. | 14 |
| MAJOR | bundle counts and cross-refs go stale after the conversions | **Applied.** BUNDLE.md is regenerated with new counts and the D3 note now points at str-t854z. No in-bucket draft still references the removed slugs. | BUNDLE.md |
| MINOR | 09: markdown shows `(n/n)`; :332 is NumericConstant | **Applied.** Evidence corrected. | 09 |
| MINOR | 10: "not silently treated as UNSAT" premise | **Partially.** The orchestrator *does* treat `Err(_)` like Unsat (orchestrator.rs:2110), so the original premise was essentially right. The AC is restated in terms of the actual code paths. | 10 |
| MINOR | 11: checkout at :4608 not :4607 | **Applied.** | 11 |
| MINOR | 02: fixture paths underspecified | **Applied.** Examples-repo paths are named (`standalone/ts/01-arithmetic.ts`, `standalone/ts/04-errors.ts`, `standalone/rust/04_errors.rs`). Other fixtures are self-contained. | 02 |
| MINOR | 03: the test must name the setup level | **Applied.** Tests are required per level. | 03 |

## Splits and conversions

- `z3-mixed-int-real-sort-split` (01, new) became `t854z-sort-split-note` (note-to-existing str-t854z). File renamed to `01-t854z-sort-split-note.md`.
- `float-constant-rational-conversion` (12, new, P3) became `aureo-float-constant-note` (note-to-existing str-aureo, proposes P1). File renamed to `12-aureo-float-constant-note.md`.
- `concolic-refine-execute-builder` (08) keeps its slug, narrowed to the refine request-field fix. The slug name is kept for stability even though the builder moved out.
  - New `concolic-refine-path-accounting` (13, P2) is blocked by 08.
  - New `execute-request-builder` (14, P3, task) is blocked by 08 and 13. Add str-qwua7.5 as a blocker when filing.
- 03 now references `execute-request-builder` instead of `concolic-refine-execute-builder` for the builder.

## Cross-bucket effects

- shatter-concolic-and-engine-design references `z3-mixed-int-real-sort-split` in `02-concolic-early-termination.md` (:69, :81, :102), `12-concolic-early-termination-fix.md` (:42, :53) and its BUNDLE.md (:193). Replace these with the existing id **str-t854z**.
- shatter-concolic-and-engine-design `06-engine-parity-e2e.md` references `concolic-refine-execute-builder` for "refine drops prepare_id/execution_profile". That slug still owns exactly that, so no change is needed. The builder and refine accounting are now `execute-request-builder` and `concolic-refine-path-accounting`, if that draft wants to cite them.
