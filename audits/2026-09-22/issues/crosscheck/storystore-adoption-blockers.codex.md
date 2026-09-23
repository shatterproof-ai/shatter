# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** d354be2f68f0596d5b2c4448b03aa6a39cf6261257e473cd263ee3eb41063031
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket storystore-adoption-blockers (storystore). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

Not ready to file as-is. I verified storystore HEAD `cca768d` and reproduced the quoted inventory counts. Live tracker deduplication remains unverified: even read-only `bd list` failed because Dolt attempted to open a lock file on the read-only filesystem.

- **MAJOR — 04: Hash scope excludes shipped runtime files.** The criteria explicitly allow hashing only `shared/` and `skills/`, but [the packaging contract](/home/ketan/project/storystore/tests/test_published_bundle.py:36) also ships scripts, examples, manifests, and other files. An installer or landing-hook change could satisfy these criteria without triggering a version bump.

- **MAJOR — 04: The staleness check can pass with broken generated outputs.** Comparing source content against a recorded hash does not establish that committed Claude/Codex payloads and manifests match those sources. Require a check that detects stale or missing generated files and inconsistent manifest versions, with a test demonstrating that failure.

- **MAJOR — 02: Independent deliverables and conflicting scope are bundled together.** Shatter’s clap adoption, builder-style clap support, fixture-only Cobra support for other consumers, and coverage-gap reporting are independently deliverable; requiring all of them unnecessarily expands the adoption blocker. The approach also says “single-file scope” while requiring cross-file enum resolution, leaving the extraction boundary contradictory.

- **MAJOR — 02: Acceptance omits updates to the published behavior contract.** [spec.md](/home/ketan/project/storystore/spec.md:475) explicitly promises no language coverage beyond TypeScript without `--thorough`, which the proposed default Rust/Go extraction would invalidate. Require updates to the canonical and packaged contract, including the nested-command naming convention and any replacement for the current language metadata.

- **MAJOR — 01/02: Filing prerequisites are incorrectly carried forward as implementation dependencies.** Migration must already be complete before these tickets exist, yet 02 remains blocked on all of 01 because “the tracker must accept writes.” That makes extractor work wait for unrelated plan-file moves and clone verification; separate migration evidence from remaining repository setup and retain only actual implementation dependencies.

- **MINOR — 04: The claim that build-plugin never changes the version is false.** [build-plugin](/home/ketan/project/storystore/scripts/build-plugin:259) already writes an incremented version when invoked with `--bump`, and README/INSTALL document that interface. Describe the missing automatic behavior accurately and specify how `--bump` and `--shared-only` behave afterward, including documentation updates.

- **MINOR — 04: “Every consumer” is unsupported by the evidence.** The inspected Claude installation supports the 128-commit lag claim for that installation, not for all consumers. Restrict the claim accordingly and distinguish the observed stale cache from the asserted cause of update failure.

**Verdict:** Revise before filing. The three highest-value fixes are to define complete payload hashing and generated-output verification, split the extractor work into bounded deliverables with an explicit contract, and remove dependencies whose prerequisite has already been satisfied before filing.
