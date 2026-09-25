# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 62b44ed3b9a923e72650b35f172717834ec3b998e4402646b71b6c24202e7d4a
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-frontend-rust (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

**Not ready to file as-is.** Read-only inspection confirmed several underlying defects, but found incorrect acceptance criteria, incomplete fixes, and misleading verification commands. Duplication checks used the tracked Beads JSONL snapshot, not the live database.

- **MAJOR — Multiple items prescribe E2E commands that silently skip the tests.** Existing `e2e_concolic_rust` tests are `#[ignore]`, so the repeated plain `cargo test --test e2e_concolic_rust` command can succeed without exercising them. Require `task e2e-rust` or the documented examples setup plus `-- --include-ignored`, and record executed test counts.

- **MAJOR — Item 01 permits a solution that violates its protocol-isolation requirement.** Sentinel-prefixed lines remain writable by user code, and an ordinary `print!` without a newline can concatenate user output with the protocol response. Require a separate protocol channel and regression cases covering partial lines, sentinel-like output, and successive requests.

- **MAJOR — Item 01’s proposed implementation ignores retained Windows support.** `std::os::fd`, POSIX `dup`/`dup2`, and `/dev/stdout` do not provide a Windows implementation; the existing standalone generator explicitly uses Unix APIs. D1 therefore does affect this bucket: require a platform-compatible design and validation.

- **MAJOR — Item 02 specifies incorrect match-arm reachability.** Earlier successful arms must be excluded from later ranges and bindings, not just wildcards: for `0..=10 => A, 5..=15 => B`, the proposed B predicate incorrectly accepts 7. Require ordered pattern-and-guard semantics, with overlapping-range and failed-guard tests.

- **MAJOR — Item 02’s property test checks the wrong wire type.** Instrumentation emits bare `SymExpr` JSON; runtime `branch_hit` adds the `SymConstraint::Expr` wrapper ([runtime source](/home/ketan/project/shatter/shatter-rust-runtime/src/lib.rs:178)). Correct emitted expressions would fail the proposed direct `SymConstraint` deserialization test; validate `SymExpr` or inspect the result after `branch_hit`.

- **MAJOR — Item 02 duplicates open str-qwua7.36 without assigning ownership.** That issue already requires the shared typed builder, serde serialization, removal of handwritten JSON, and property tests. Add escaping reproductions there, then scope the new issue to match semantics with an explicit dependency; `blocked_by: []` contradicts “land with or on top.”

- **MAJOR — Item 03 assumes an enforced build deadline that does not exist.** The executor checks elapsed time only after blocking `Command::output()` returns, and crate-bridge can attempt multiple builds ([executor](/home/ketan/project/shatter/shatter-rust/src/executor.rs:2990)). Increasing the request timeout to one build budget plus execution time cannot guarantee the stated invariant; specify subprocess deadline enforcement and aggregate fallback-build budgeting.

- **MAJOR — Item 03’s cold-cache proof does not establish a cold harness build.** Harness compilation explicitly replaces `CARGO_TARGET_DIR` with its own selected directory, including paths under `SHATTER_HARNESS_CACHE`. Specify isolated harness caches and evidence that compilation actually occurred.

- **MAJOR — Items 05/06 misidentify the known unsigned-integer gap.** The shared `int_range` helper deliberately returns no bounds for 64- and 128-bit integers, including `usize`; generation, mutation, and solver paths consult that helper ([types.rs](/home/ketan/project/shatter/shatter-core/src/types.rs:310)). Document this concrete limitation, and coordinate item 11’s validator with its correction rather than recommending unchanged reuse.

- **MAJOR — Items 05/06 lack a reproducible, accessible regression artifact.** Neither the referenced `18_accept_language.rs` fixture nor `rust-walk.md` transcript exists in commit `56c86168`, and “current release” identifies neither executable build. Attach a minimal fixture, exact binary identities, and a complete reproduction before asserting that the closed fix regressed.

- **MAJOR — Item 04 offers an alternative that cannot satisfy its acceptance test.** Replacing Mutexes with owned HashMaps removes poisoning but cannot keep the process alive after an uncaught dispatch panic. Make the panic boundary mandatory for the proposed recovery behavior, or define separate acceptance criteria for the owned-cache alternative.

- **MAJOR — Item 08’s proposed cleanup misses other silent-pass paths.** Besides the 15 named guards, `cargo_build_unavailable` accepts any error containing `"cargo"` or `"No such file"` and has additional callers ([executor](/home/ketan/project/shatter/shatter-rust/src/executor.rs:11566)). Inventory actual skip branches; 38 textual occurrences of “skipping” are not evidence of 38 distinct affected tests.

- **MAJOR — Item 08’s offline experiment can fail before testing the defect.** An empty `CARGO_HOME` with offline mode can prevent compilation of the outer test suite altogether. Use a prebuilt test executable or controlled fixture-build failure, and demonstrate that the previously successful early-return branch was reached.

- **MAJOR — Item 09’s suggested fingerprint can retain stale type information.** Maximum mtime plus file count misses changes whenever those aggregate values remain unchanged, including edits to an older file while another file has a later timestamp. Require per-file change detection and tests covering edits, replacements, additions, and removals.

- **MAJOR — Item 10 omits a parallel malformed-request path.** The generated crate-bridge loop also defaults malformed JSON and missing inputs in `executor.rs:5179–5182`. Include it or explicitly track its exclusion; otherwise the promised protocol-error behavior remains routing-dependent.

- **MAJOR — Several tickets combine independently completable work.** Item 09 joins registry caching with extractor classification; item 11 joins credential redaction, parsing, and retry scheduling; item 12 adds runtime provider diagnostics to documentation work. Split these into focused tickets with independent acceptance criteria and realistic estimates.

- **MAJOR — Item 11’s shared-secret placement creates a dependency cycle.** Putting the wrapper in `shatter-llm` and importing it into `shatter-core::config` reverses the existing production dependency from LLM to core. Put the shared type in core or a dependency-neutral location, or use manual `Debug` implementations.

- **MINOR — Items 03/08 mischaracterize str-jyxr.** Its tracked scope concerns frontend `generate` requests consuming input-prefetch budgets, not Cargo dependency fetching. Correct the references so implementers do not expect it to supply fixture dependency prefetching.

- **MINOR — Item 12 overstates missing model-rejection diagnostics.** The OpenAI adapter already emits `OpenAI model not found (404)` ([openai.rs](/home/ketan/project/shatter/shatter-llm/src/openai.rs:132)). Identify the deficient provider or CLI propagation path and reproduce it before requiring new diagnostic behavior.

The top three improvements are: replace false-green verification with executable checks; correct the protocol, match, timeout, and integer-bound requirements; and reconcile overlapping work into focused tickets with durable reproductions.
