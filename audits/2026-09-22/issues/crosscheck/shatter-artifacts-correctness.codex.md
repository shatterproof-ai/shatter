# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 0ee4f638b840d9e1ff13da90ffe897dcf1c6a82c184817d3f575cb91d186002d
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-artifacts-correctness (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

Reviewed against `56c86168`, relevant newer `main` changes, and the local tracker export. Live tracker status and runtime reproductions were not verified.

- **MAJOR — 01: Partial resume still lacks source validation.** `PersistedExploreState` stores paths and inputs without a source fingerprint, and `read_resume_state` loads it without checking source changes. Adding only the prescribed options hash leaves stale partial state reusable after code edits; require deep-fingerprint validation and an interrupted-run/source-edit test.

- **MAJOR — 02: Failed-function output requires an undefined schema change.** The acceptance criteria require functions to be recorded “as failed with a failure class,” but `FunctionSpec` has no execution-status field and `FileSpecBundleStatus` contains only `Ok` and `NoTargets`. Define the failure representation, versioning, and consumer behavior, distinguishing exploration failures from observed target exceptions.

- **MAJOR — 03: Multi-file comparison can still collide on function names.** `diff_spec_collections` indexes specs solely by `function_name` (`commands/diff.rs:225–252`). An envelope loader could satisfy the proposed distinct-name tests while comparing the wrong functions; require file-qualified matching and a test with identically named functions in different files.

- **MAJOR — 03: `compare` support is a larger, undefined change.** `commands/compare.rs:19–22` reads individual `FunctionSpec` objects; it does not currently consume `--spec-out` bundles as claimed. Specify how functions are paired across multi-file inputs, and reconcile ownership with the referenced `spec-json-shapes-compare` work.

- **MAJOR — 04: Checkpoint acceptance contradicts supported CLI modes.** A normal scan without `--resume` does not write a checkpoint, while `--resume PATH` deliberately uses the supplied destination (`commands/scan.rs`). Scope the unified-location requirement to `--resume auto`, explicitly enable that mode in the integration test, and preserve explicit-path behavior.

- **MAJOR — 05: The suggested error-mask key is wrong.** Existing nondeterminism metadata uses `thrown_error`, not the proposed `error` prefix (`nondeterminism.rs:490`). Define comparison using the actual mask vocabulary and error fields; otherwise persisted masks will be ignored, and comparing unstable stack locations could introduce false regressions.

- **MAJOR — 11: The proposed denominator excludes lines counted by the runtime.** Rust’s runtime adds branch-decision lines to `lines_executed`, including match-arm pattern lines emitted through `branch_hit`; the draft counts only `line_hit` lines. Require the union of all probe locations contributing to the numerator, with a multiline match-arm regression test.

- **MAJOR — 01/10/11: Required E2E commands can pass without exercising the fixes.** The frontend E2E suites contain ignored tests, including the Go denominator test explicitly cited by draft 10. Replace the bare Cargo commands with the repository’s appropriate E2E tasks, which supply prerequisites and `--include-ignored`, and require evidence that the named regression test executed.

- **MAJOR — 14: A required acceptance test depends on unfinished work without an ordering rule.** The whole-stdout `explore --spec-json` test depends on open `str-qwua7.11`, yet the note declares no blocker and merely says “once” that fix lands. Add the dependency or assign that test to `.11` so `.39` has independently checkable completion conditions.

- **MINOR — 02: The artifact-only JSON behavior is misstated.** In `finalize_explore`, the `-o .json` branch writes nothing when `acc.file_specs` is empty; the no-target fallback belongs to the separate `--spec-out` branch. Correct the evidence and test both sinks explicitly.

- **MINOR — 03: Existing spec-diff compatibility is described incorrectly.** Its loader accepts one `FunctionSpec` object or a `FileSpecBundle`, not a bare list of function specs. Correct the supported-input inventory before extending it.

- **MINOR — 11: Execute-response support adds unexplained protocol scope.** The core defines `instrumentable_line_count` on Instrument responses, not `ExecuteResult`. Restrict the requirement to Instrument responses or explicitly describe the new Execute field, consumer, and parity implications.

- **MINOR — 07: The zero-reference search matches unrelated types.** `Snapshot::` also matches existing `SourceFileSnapshot::path` and `SourceFileSnapshot::line_count` calls. Tighten the expression and define exclusions for historical documentation so completion does not require unrelated edits.

- **MINOR — 14: “Every JSON stdout command” is not the listed inventory.** Existing `compare --json` and `revalidate --json` are omitted. Either include them or state that the test scope is intentionally limited.

**Verdict: not ready to file as-is.** The highest-value fixes are to define the JSON failure and multi-file identity contracts, correct the resume/coverage/checkpoint acceptance criteria against actual code, and replace ambiguous test dependencies and skipped-test commands with executable completion checks.
