# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 382f99ee2505a22114a6bb3048abef5f23471473bc09cec49fa2de990a65c848
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-concolic-and-engine-design (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

**Not ready to file as-is.** The strongest problem is a contradicted diagnosis in draft 02; several other drafts have acceptance criteria that permit misleading results or leave substantial scope unresolved.

1. **BLOCKER — 02: The artifacts contradict “zero solver contribution.”**  
   The cited artifacts contain `z3` discoveries for `classifyHttpResponse`, `authorizeRequest`, `validateEmail`, and others. `ExploreResultAccumulator` in `shatter-cli/src/commands/explore.rs:190–330` discards `solver_guided_inputs` and `stop_reason`, then reconstructs them through `Default`; separate this confirmed reporting bug from the unproven claim that the engine drains its worklist without solving.

2. **MAJOR — 01–03: Essential reproduction evidence is not available from a clean checkout.**  
   The cited `ts-sub-concolic` reports and `ts-concolic-fresh` artifacts exist locally but are not tracked in the inspected worktree. Attach durable evidence and provide the complete command, examples revision, function manifest, configuration, and executable revision so a fresh agent can reproduce the comparison.

3. **MAJOR — 01: The proposed scan route does not satisfy the benchmark controls as written.**  
   `scan` has no `--clean`; it has separate cache and resume controls, and its concolic fuzz phase uses `StdRng::from_os_rng()` despite a supplied seed (`orchestrator.rs:3060`). Specify working cache isolation and RNG controls, plus how equal execution budgets are enforced despite scan’s 5× concolic allowance, before declaring this dependency-free.

4. **MAJOR — 02: The regression criterion rewards extra executions rather than correct solving.**  
   Requiring the fixture to exceed 21 executions rejects a correct solver that reaches its target sooner, while permitting every early exhaustion to be excused in prose makes the broader criterion weak. Require a reproducible missed branch, verified solver provenance, accurate termination reporting, and an explicit expected result under a bounded budget.

5. **MAJOR — 01/03: Nobody owns the required post-fix benchmark run.**  
   Draft 01 can close before draft 02, but draft 03 requires numbers measured after draft 02 lands and explicitly excludes running the benchmark. Assign that rerun to a named deliverable and encode its ordering so both prerequisites closing actually makes the decision executable.

6. **MAJOR — 05: Equal discovered path counts do not prove equal path identity.**  
   Random and concolic exploration can legitimately discover different subsets under finite budgets even when they use identical path canonicalization. Test canonical identities against the same execution traces, including loop buckets and fallback cases; use separately specified coverage expectations for engine-level tests.

7. **MAJOR — 05: Existing issue ownership remains unresolved.**  
   The exported tracker records already assign shared configuration construction to `str-qwua7.6.2` and relocation of `hash_branch_path` to `str-qwua7.29`; draft 05 introduces overlapping acceptance criteria while leaving the implementer to choose where work belongs. Reconcile these into explicit notes, dependencies, or superseded scope before filing; the export alone does not verify current live status.

8. **MAJOR — 04, 06, 07: Several drafts bundle independently deliverable projects.**  
   Draft 04 combines benchmark implementation, another repository’s disposition, and tracker initialization/backlog migration; draft 06 combines E2E coverage, a repository-wide lint, and completion policy; draft 07 combines deletions with new analysis tooling. Split these before filing rather than asking the eventual assignee to discover the task boundaries.

9. **MAJOR — 04: “Bug-finding effectiveness” has no defined scoring oracle.**  
   The criteria allow “expected outcomes found” without specifying the seeded faults, observable failure condition, denominator, or treatment of false positives and unsupported targets. A branch-coverage report could therefore satisfy the ticket while leaving its stated effectiveness question unanswered.

10. **MAJOR — 06: Expected failures need executable failure semantics.**  
    “Marked expected-fail” does not specify whether assertions execute, whether unexpected passes fail, or whether unrelated failures remain visible. Require narrowly scoped expected failures with strict unexpected-pass detection; otherwise an ignored test or blanket failure allowance can keep the suite green indefinitely.

11. **MAJOR — 07: Outside-file reference counting is not production reachability.**  
    Mutually referencing dead modules can pass the proposed gate, while a live public helper called only within its own file can fail it. Define production roots and graph traversal, or explicitly describe a reference heuristic with regression cases covering these limitations.

12. **MAJOR — 08: “Every persisted hash” exceeds the stated inventory.**  
    Additional qualifying sites include the external-audit cache path in `shatter-cli/src/helpers.rs:711` and persisted scope hashing in `shatter-cli/src/commands/run.rs:1041`. Enumerate the full migration surface and exclusions so the implementer can distinguish a bounded sampling/cache fix from a workspace-wide compatibility change.

The top fixes are: correct draft 02 around the confirmed accumulator bug; make the benchmark reproducible with durable evidence and explicit post-fix ownership; and reconcile overlapping scope into independently completable tickets.
