# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** ceb590fcc0310fa3d9334fa3e808dacda09561537016bc219b25e8efbf13ade5
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-gates-integrity (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

**Not ready to file as-is.** The core problems are supported by the code, but several proposed fixes would still permit misleading evidence or conflict with existing work. Review was read-only against main `70465921` and the local tracker export.

1. **MAJOR — #02’s guard does not prove that required leaves executed.** A grep rejecting `is up to date` still passes when a required leaf disappears from the task graph entirely. Require positive evidence for an explicit expected leaf set, with tests for absent leaves and failed commands; replace the existing CI invocation rather than adding a second `task check` “post-step.”

2. **MAJOR — #03’s `touch` acceptance test cannot prove checksum invalidation.** Task’s content checksum does not change when only modification time changes, so the specified proof can fail after a correct fix. Prime the cache with a successful run, change file contents, then verify execution without forcing it.

3. **MAJOR — #06’s new discovery guard can itself be cached away.** `meta.sources` enumerates existing files, so adding an otherwise unreferenced test module need not invalidate the task containing the guard. Require source patterns covering future test additions and a regression that adds an unwired module after priming the cache.

4. **MAJOR — #06 treats intentionally staged infrastructure as disposable dead code.** The tracker assigns pressure-provider/event-log integration to `str-35vtk.19`, and `str-35vtk.25` depends on the event store. Requiring these helpers to be integrated or deleted expands test wiring into separately owned functionality and risks removing prerequisites.

5. **MAJOR — #06 and #07 permit incompatible completion states.** #06 requires tests reachable from `check`, while #07 allows checker tests to run only under `gauntlet`, which `check` does not invoke. Choose a consistent destination and record the dependency where #06 relies on #07’s rewrite.

6. **MAJOR — #07 omits the JSON fields that actually represent interruptions.** `report.rs` places interrupted functions in `codebase.skipped_functions[]` with `category == "interrupted"`, not `codebase.failed[]`. The proposed implementation and four-failure proof can miss all seven interruptions; require an interruption-only fixture and explicit handling of intentional timeout and dry-run steps.

7. **MAJOR — #09’s timeout criterion would not configure the installed runner.** Bento 2.3.73’s `land-work-run-verifier.py` obtains its timeout from `--timeout`, and `land.py` forwards that argument; the inspected first-party source behaves likewise. Merely adding a timeout field to `verifier.json` does not satisfy an enforced-timeout requirement; specify supported wiring and a runtime timeout test.

8. **MAJOR — #09 duplicates existing acceptance criteria without reconciling ownership.** `str-qwua7.55` already owns verifier honesty and timeout, `str-qwua7.2` owns execution reporting, and `str-35vtk.24` requires exactly one `task check`. The proposed companion note does not transfer those criteria, and allowing `task affected` conflicts with `.24`; explicitly revise or partition the existing tickets.

9. **MAJOR — #09’s execution flag is undefined for mixed results.** A composite command can execute required tests while dependencies print `is up to date`; counting those lines cannot establish which required leaves ran. Define execution evidence per required leaf and test mixed, missing, entirely cached, and failed outcomes rather than only one cached stub.

10. **MAJOR — #10 assumes telemetry granularity that does not exist.** `gate-wrapper.sh` records one row per outer invocation and bypasses nested recording through `SHATTER_GATE_LOCK_HELD`; adding a column cannot produce per-leaf identities or durations. Define the event schema and aggregation, and require new measurements because historical CSV rows cannot retrospectively distinguish execution from caching.

11. **MAJOR — #05’s suggested replacement still builds frontends.** `shatter-cli/build.rs` invokes npm bundling and Go compilation, so replacing tests with `cargo check` or clippy still triggers frontend builds on a cold CLI checkout. Clarify whether “none … needs a built frontend” means no prebuilt artifacts or no frontend build activity, and specify a reproducible cache/dependency setup for the latency proof.

12. **MAJOR — #05 misidentifies both existing governance and receipt prerequisites.** Pre-push already reaches `gate-wrapper.sh` through `task check` or `task affected`, while `str-35vtk.25` explicitly implements shadow checking that preserves the real gate. Scope governance changes to uncovered paths and reconcile reuse with `str-35vtk.26`’s observation requirement instead of treating `.24/.25` as sufficient.

13. **MAJOR — #01’s closure proof can depend on its blocked follow-up.** `check` is staged and fails before later stages when an earlier test fails, yet #01 requires every stage-2/3 leaf to execute while assigning newly exposed failures to #02, which is blocked by #01. Permit isolated leaf evidence or otherwise separate execution proof from resolving failures so the dependency can close.

14. **MAJOR — The snapshot-equivalence claim is false, and mandatory evidence needs retrieval instructions.** Comparing `56c86168` with main shows changes to `shatter-core/src/cache.rs` and `scan_orchestrator.rs` as well as the Taskfile; the cited September 22 artifacts are also absent from main. Qualify which files were reverified and provide commit-qualified retrieval instructions or attachments for required `scan-mix.*` and gate logs.

The top fixes are to define positive per-leaf execution evidence, reconcile ticket ownership and dependencies, and replace invalid proofs with reproducible tests against the actual runtime contracts.
