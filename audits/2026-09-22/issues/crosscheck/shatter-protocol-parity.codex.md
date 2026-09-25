# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 488b9f6ec01038aedcd7fe8b9542f11ede3c6f06d96f969002556174cc4f413a
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-protocol-parity (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

The bundle is **not ready to file as-is**. Several underlying defects are supported by the code, but some acceptance criteria permit ineffective fixes or conflict with existing gates. Live `bd` access failed in this read-only environment; current tracker statuses remain unverified.

- **MAJOR — Entries 03 and 15 duplicate ownership of existing table work.** Both require replacing the skill’s capability tables, while entry 03 also claims the crate tables already assigned to str-qwua7.24. “Coordinate” leaves multiple independently claimable issues owning the same deliverable; assign ownership explicitly and narrow the others.

- **MAJOR — Entry 02’s restart strategy loses prerequisite state.** `analyze_runtime_value_go` precedes `planner_runtime_value_go`, whose handler requires cached analysis ([handler.go](/home/ketan/project/shatter/shatter-go/protocol/handler.go:1958)). Restarting and handshaking after the analyze timeout therefore causes another failure; define prerequisite replay, isolated cases, or dependent-case skips before promising “exactly one failure.”

- **MAJOR — Entry 02’s regex fix does not repair the actual drift comparison.** [`extract_structure()`](/home/ketan/project/shatter/protocol/conformance/conformance_harness.py:223) replaces string values with `"string"` and inspects only the first array element, erasing the discriminator values referenced by patterns such as `condition.*ite`. Require tests using actual comparator output and populated constraints/side effects; testing `re.search` against a fabricated drift string is insufficient.

- **MAJOR — Entry 01’s runtime coverage can pass without successful implementation.** Existing setup/generate cases accept error responses, and the harness skips commands absent from advertised capabilities; merely requiring a case for each pair permits both loopholes. Define coverage as an executed, successful command-specific assertion with prerequisites—TS prepare, for example, requires prior instrumentation.

- **MAJOR — Entry 06’s deletion option breaks the existing parity gate.** [`validate_divergence_metadata()`](/home/ketan/project/shatter/scripts/validate-parity.py:579) hard-fails when matrix divergence IDs lack matching PARITY.md headings. Deleting the mirror and adding a link must explicitly include changing that validator and its tests; the deletion option currently requires only grep evidence.

- **MAJOR — Entries 03 and 04 specify canaries that already succeed today.** A matrix-only complex-type change already fails the existing handshake comparison, and a nonlegacy enum addition already fails codegen checking through the manifest, which contains every enum. Require isolated mutations proving the newly added registry/golden checks and each language emitter’s coverage.

- **MAJOR — Entry 04 can generate unused constants and leave frontend drift intact.** Go/Rust generated artifacts are vocabulary arrays, while handwritten wire definitions remain separate; emitting all thirteen arrays plus checking core serde satisfies the acceptance criteria without checking those frontend definitions. Require actual adoption or frontend synchronization tests for every newly covered enum.

- **MAJOR — Forced gate runs conceal missing cache invalidation.** [`parity.sources`](/home/ketan/project/shatter/Taskfile.yml:252) omits the matrix, PARITY.md, and `validate-parity.py`, yet the drafts repeatedly validate only forced execution. Add source/dependency wiring and a check that an ordinary invocation reruns after changing each newly governed input.

- **MAJOR — Entry 07 requires an enforcement test that it neither owns nor depends on.** It must name a test enforcing registry field-model parity with core serde, but that broader check remains work described by str-2fjn and is outside this documentation issue’s scope. Name an existing qualifying test, add the dependency, or explicitly document the current enforcement gap.

- **MAJOR — Entries 02 and 03 remain oversized, independently splittable tasks.** Entry 02 combines transport recovery, divergence policy, summary reporting, and cross-language fixtures; entry 03 remains explicitly L-sized after the codegen split. These exceed the repository’s focused, single-session issue guidance and need smaller deliverables with clear ownership.

- **MINOR — Entry 04 misstates enum coverage.** The language emitters cover **three of thirteen** entries under `enums:`—setup level, generator kind, and branch type. Commands, statuses, and error codes are three additional vocabularies from separate registry mappings.

- **MINOR — Filing metadata is internally inconsistent.** The index contains fourteen `kind: new` entries, despite the header saying thirteen; entry 11 uses `kind: reopen-note` while explicitly prohibiting reopening. Correct the count and make the machine-readable action unambiguously comment-only.

The top fixes are to reconcile ownership and dependencies, replace already-passing or superficial proofs with tests that distinguish the intended fixes, and specify state recovery plus validator/cache changes needed to keep the resulting gates reliable.
