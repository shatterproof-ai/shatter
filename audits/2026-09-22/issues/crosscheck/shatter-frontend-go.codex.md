# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 84b0af6e8b5f645152498fe964b5a8dbd18ba7fc652307904d9e8dbe173c970c
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-frontend-go (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

Reviewed the cited source at `56c86168`. I did not independently verify current live-tracker duplication.

1. **MAJOR — 02/03: The nested-module tag requirement is incorrect.** `continuous-*` tags are revision queries, which Go can resolve to pseudo-versions; module-directory prefixes are required for semantic-version tags. Remove the mandatory prefixed-tag change and correct the companion comment. [Go module reference](https://go.dev/ref/mod#version-queries).

2. **MAJOR — Multiple drafts: The prescribed E2E command skips the tests.** The Go E2E cases are marked `#[ignore]`, so `cargo test --test e2e_concolic_go` can pass without executing them. Require the existing `task e2e-go` gate and evidence that the relevant cases actually ran.

3. **MAJOR — 01: Required release proof has an undeclared dependency.** Closure requires an entirely green release workflow, while known Windows/aarch64 failures belong to another issue and `blocked_by` remains empty. Encode that release-workflow dependency so this issue does not appear independently completable.

4. **MINOR — 01: The release smoke does not establish relocation safety.** Changing the working directory leaves the compiled-in checkout path accessible on the same runner, allowing the original implementation to pass. Run the staged artifact where its build checkout is unavailable, with a fresh workspace/cache.

5. **MAJOR — 04: Acceptance requires behavior explicitly excluded from scope.** The concolic escaped-string test must reach its expected arm, although the draft reports that the runtime already emits the correct tab and excludes investigating the remaining miss. Separate literal-decoding acceptance from the unresolved concolic defect, or add the necessary dependency.

6. **MAJOR — 06: The boundary rule leaves standalone files exposed.** Go supports markerless standalone files through `loader.LoadFile`; those still walk through `/tmp` to `/` under the proposed marker-based stopping rule. Adding a marker only to the failing test masks this supported user scenario; specify and test markerless-file behavior.

7. **MAJOR — 08: Atomic extraction does not repair existing corrupt cache entries.** The required truncated-cache recovery test cannot be satisfied by temp-file extraction alone because `isExecutable` accepts an existing truncated executable before extraction begins. Define cache validation or migration, including how existing entries are trusted or replaced.

8. **MAJOR — 08: Unconditional download authentication can disclose the token.** `downloadFile` receives a manifest-provided asset URL, yet acceptance requires sending `GITHUB_TOKEN` whenever set. Restrict credentials to explicitly trusted origins and test that external asset URLs and redirects receive no token.

9. **MAJOR — 09: Default timeout ordering contradicts session preservation.** Both request and build timeouts default to 30 seconds, and the outer request timeout taints the frontend session (`frontend.rs:329–342`). A later-starting build timer cannot reliably report its target-level timeout first; address the deadline relationship instead of treating this as Rust-only work.

10. **MAJOR — 09: The suggested process cleanup omits Windows.** Unconditional `SysProcAttr.Setpgid` and Unix group killing will not compile for the Windows release target retained by D1. Require platform-specific process-tree cleanup and Windows validation.

11. **MAJOR — 11: The proposed regression can pass without exercising cold builds.** `Workspace.GoEnv()` overrides caller-supplied `GOCACHE`, so an empty external cache does not establish the intended conditions. Isolate the actual workspace and launcher caches, and assert first-build serialization followed by parallelism; quiet-host success alone does not distinguish fixed behavior.

12. **MAJOR — 10/13: Implementation versus documentation remains an unresolved scope decision.** These tickets permit closure either by changing runtime behavior or by documenting its absence, with substantially different outcomes and estimates. Select the deliverable before filing, or make each a bounded investigation with explicitly tracked implementation follow-up.

13. **MINOR — 12: Four independent defects are bundled as one task.** Coverage records, mock source generation, unused assignments, and lock-write handling have separate failure modes and verification. Split the coverage and mock-generation fixes from housekeeping.

**Verdict: Not ready to file as-is.** The most valuable fixes are:

- Correct the module-version claim and make regression gates actually exercise the reported failures.
- Resolve contradictory scope and encode the release/timeout dependencies.
- Specify the missing boundary conditions: standalone files, existing corrupt caches, and Windows process cleanup.
