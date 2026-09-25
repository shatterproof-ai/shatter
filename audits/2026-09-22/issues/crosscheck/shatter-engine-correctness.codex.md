# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** a81f228a7d6f49046c5d064b66fd727327c7c41fe5a03068bc65f54c3864ef8e
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-engine-correctness (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

**Not ready to file as-is.** The source supports several underlying defects, but the bundle contains incorrect reproduction instructions and conflicting acceptance criteria. I checked repository code and the available tracker export; I did not rerun runtime repros or verify live tracker state.

1. **BLOCKER — 03/04: The alleged `--setup` flag does not exist.**  
   `args.rs` defines `--setup-timeout` and `--fail-on-setup-error`, but setup files come from configuration; [explore.rs:5051](/home/ketan/project/shatter/shatter-cli/src/commands/explore.rs:5051) reads `resolved.setup`. Rewrite both the issue and closed-ticket comment around a reproducible `.shatter/config.yaml` example—the stated command cannot demonstrate silent flag loss.

2. **MAJOR — 05: The permitted resolutions contradict the mandatory test.**  
   The first criterion permits retaining fixed mocks with documentation and a warning, while another requires a mock-dependent fixture to reach both branches through concolic variation. Choose restoration as the required outcome, or define separate acceptance tests for the warning-only alternative.

3. **MAJOR — 12: Float corruption starts far below the stated threshold.**  
   The pinned Z3 0.19.10 wrapper casts `Real::from_rational` arguments to C `int`, so the scaled numerator already overflows around **2147.483647**; for example, 3000.0 becomes −1294.967296. Correct the evidence and reconsider the P3 priority using this substantially broader affected range.

4. **MAJOR — 12: “Shortest round-trip decimal” does not satisfy exact rational conversion.**  
   Binary f64 `0.1` is exactly `3602879701896397/36028797018963968`, whereas decimal `"0.1"` represents `1/10`; both round-trip to the same f64. Remove that alternative and test exact rational equality, since the proposed round-trip assertion can approve an incorrect implementation.

5. **MAJOR — 12: Mandatory tests require explicitly excluded extraction work.**  
   Exact rationals for tiny values can exceed the integer sizes supported by `as_rational()`, after which [solver.rs:1038](/home/ketan/project/shatter/shatter-core/src/solver.rs:1038) tries parsing Z3’s rational expression as an f64 and can omit the assignment. Include model extraction in scope, or separate translation tests from model-round-trip tests and declare the dependency.

6. **MAJOR — 01: The prescribed E2E commands can pass without exercising the regression.**  
   The subprocess E2E tests are marked `#[ignore]`; plain `cargo test --test e2e_concolic_go` and `cargo test --test e2e_concolic` skip those tests. Require the appropriate `task e2e-*` targets or explicit ignored-test execution, with evidence that the named regressions actually ran.

7. **MAJOR — 01: Fixture instructions omit the external examples repository.**  
   The test helpers resolve fixtures through `SHATTER_EXAMPLES_DIR` or `/tmp/shatter-examples-main`, including `standalone/go` and `standalone/ts`, rather than the specified local `examples/go/` location. Identify the owning repository and coordinated changes, or explicitly choose self-contained test fixtures.

8. **MAJOR — 03: The lifecycle proposal leaves per-execution setup undefined.**  
   The existing explorer supports both function-level and execution-level setup, with execution teardown inside the loop ([explorer.rs:1632](/home/ketan/project/shatter/shatter-core/src/explorer.rs:1632)). A pipeline helper that sets up once and tears down after shrinking does not preserve execution-level semantics; specify and test lifecycle behavior for shrink attempts and both setup levels.

9. **MAJOR — 08: Scope and ownership conflict with the capture ticket.**  
   This entry combines refine-request repair, refine-result accounting, and migration of every Execute site across both engines, while allowing closure of `str-qwua7.5` despite declaring capture semantics out of scope. The existing capture ticket also treats refinement as discarded probes, which this draft changes; reconcile that policy and assign explicit ownership before parallel implementation.

10. **MAJOR — 11: The proposed exhaustion predicate can reject successful recovery.**  
    A failed replacement can temporarily reduce `live_count` to zero before `maybe_grow` successfully spawns another worker. Define terminal exhaustion relative to outstanding recovery attempts, ensure blocked checkouts wake, and require a successful-retry regression alongside persistent-failure coverage.

11. **MINOR — 09: Optional range inference leaves part of “done” undefined.**  
    “Add observed min/max bounds or explain why not” allows materially different implementations, and observed extrema do not establish general bounds beyond sampled traces. Defer or separate range inference, and reconcile ownership of the source-trace invariant property already specified in `str-qwua7.47`.

12. **MINOR — 10: The suggested timeout implementation names a nonexistent variant.**  
    `SolveResult` contains `Sat` and `Unsat`; unknown results are `SolverError::Unknown`, currently discarded with other solver errors in the strategy layer. Point the implementer to that propagation path instead of an orchestrator `SolveResult::Unknown` handler.

The three highest-value fixes are: correct the reproduction and verification instructions; resolve contradictory acceptance criteria, especially numeric exactness and mock behavior; and narrow or explicitly coordinate work that overlaps existing tickets.
