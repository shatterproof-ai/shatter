# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 32c6be29653fe84cf5549876137053c4435658343fc2eb79dc56acdbb18e7cc7
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-reports-and-specs (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

Not ready to file as-is. I checked the referenced code revision and available audit artifacts; tracker comparisons used the local JSONL snapshot, so current tracker status remains unverified.

1. **MAJOR — 06/07 require a regression verdict the fixtures cannot establish.** The old `grade` bundle records `pass` at input 50, while the new bundle records `overflow` at 101; neither establishes both outputs for the same input. Because `spec-diff` currently reads files without executing targets, specify replay against source revisions or supply shared witnesses, and retain an inconclusive result when evidence is missing.

2. **MAJOR — 06 assumes every class has usable symbolic constraints.** `SymConstraint` permits `Unknown`, and `equivalence.rs:31–40` discards constraints when constructing `BranchPath`. The issue needs explicit requirements for retaining constraints, negating untaken branches, and representing incomplete knowledge; a constraint printer alone cannot satisfy “each class” having symbolic preconditions.

3. **MAJOR — 06 duplicates comparison work it explicitly excludes.** Its acceptance criteria require cross-version example matching or replay, while behavior-based pairing is declared out of scope and assigned to str-qwua7.38. Assign regression comparison to one issue and express the dependency instead of giving two agents overlapping ownership.

4. **MAJOR — 02/06 contradict the stated compatibility decision.** The bundle says D2 requires these changes to keep reading old bundles, but both tickets permit dropping compatibility with a documented error. Remove that escape clause and define legacy fixtures, mixed-version behavior, and ownership of the coordinated schema migration.

5. **MAJOR — 01’s “same run” evidence comes from different functions.** `ts-spec-invariants.md` describes `classifyNumber`, but `ts-spec-invariants.json` describes `categorizeUser`, explaining the cited `input.age` label. Replace this with matching artifacts before claiming a verified cross-format comparison.

6. **MAJOR — 04 permits a solution that fails its own acceptance test.** Renaming the existing metric to “branch points reached” legitimately preserves `2/2` when both sites are reached but one side is missed. Choose side coverage or site-count terminology, and make the required assertions consistent with that choice.

7. **MAJOR — 03 treats stable identifiers as display paths.** `report.rs:448–457` documents that `qualified_id` matches call-graph identifiers and internal lookup keys. Making it relative requires a coordinated identity/consumer migration and compatibility checks, beyond simply cleaning up rendered paths.

8. **MAJOR — 05 leaves the shared reader’s data contract undefined.** Flattening bundles into `Vec<FunctionSpec>` loses file identity and schema versions, creating ambiguity for duplicate function names across files and removing information used by existing version checks. Also specify whether properties YAML is supported: `spec.rs:680–714` uses a distinct, serialize-only invariant representation, so accepting a bundle list does not itself make that producer consumable.

9. **MAJOR — 08 lacks dependencies needed to pass.** Successful `--spec-out → compare` and equal HTML/markdown path counts require fixes 05 and 03, yet only the gauntlet-checker dependency is declared and individual bug fixes are out of scope. Add those prerequisites or define temporary expected failures explicitly.

10. **MAJOR — 08/09 lack a reproducibility contract for mandatory gates.** Existing `scan_seed_reproducibility.rs` explicitly warns that fixed seeds do not eliminate scheduling and timeout variation. Specify fixture revisions, execution budgets, cache isolation, and baseline policy; stripping timing text cannot stabilize which behaviors exploration discovers.

11. **MAJOR — 09 never defines an executable “outcome.”** The `EXPECTED BRANCHES` comments describe behavioral predicates, including continuous numeric outputs and identical errors from different variants. Neither unique output counts nor branch/path counts directly represents those expectations, so define the oracle and distinguish expected coverage from the recorded ratchet baseline.

12. **MAJOR — 09 bundles separate tasks and overlaps existing ownership.** Corpus expectation parsing, allowlist governance, and TS discriminant preservation are independently deliverable changes; “split at pickup if preferred” leaves scope unresolved. The local tracker snapshot also assigns issue-linked allowlist policy to str-qwua7.10, requiring reconciliation before filing another owner.

13. **MAJOR — 10 adds an unspecified configuration feature.** The draft leaves the configuration file, glob base, match precedence, and interaction with `policy_excluded` undecided, although `config.rs:465–471` deliberately separates scan-global JSON configuration from per-function YAML configuration. Split the override feature or provide its complete contract, including scans started below the project root.

14. **MAJOR — 12’s proposed property test varies the wrong field.** Input strings already receive JSON escaping; the demonstrated vulnerability is independently supplied `thrown_error`. Require arbitrary error/free-text fields or an explicit echo-error fixture, with NUL, ESC, DEL, and multiline cases, to establish the promised failing-before/passing-after test.

15. **MINOR — 08 incorrectly describes an existing insta setup.** The inspected snapshot tests use custom file-comparison helpers, and the checked manifests contain no insta dependency. Correct the suggested approach so implementation does not begin by searching for a nonexistent framework.

The top fixes are to define evidence and compatibility contracts for spec changes, split overlapping deliverables with explicit dependencies, and repair the fixtures and test criteria so each ticket has reproducible done-conditions.
