# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** a56658c7d0e11258d6872e3142517c2785e604efeb3ded1e17d5129e2b7b4872
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket bugshot-tracker-and-payload (bugshot). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

**Not ready to file as-is.** Repository HEAD matches `e622d73`. Live tracker verification was blocked by Dolt requiring a lock-file write; tracker comparisons below use the saved JSONL, not confirmed current database state.

- **MAJOR — 01: Cleanup proof cannot establish its acceptance criteria.** The supplied Python script only counts prefixes; it checks neither statuses nor dependency endpoints. The export also omits `--limit 0`, despite installed `bd list` documenting a default limit of 50, so the inventory needs an explicitly unlimited export and executable assertions.

- **MAJOR — 01: Dependency migration instructions are incomplete.** `bd dep list` defaults to outgoing dependencies, while incoming references also require migration; the saved tracker already contains equivalent edges on canonical twins. Specify mapping both endpoints, preserving edge types, deduplicating existing edges, and checking incoming and outgoing references before removing anything.

- **MAJOR — 03: A single-repo investigation expands into cross-repository implementation.** Its mandatory fix outcome includes choosing a publication mechanism, changing the bento marketplace, bumping versions, and adding release checks. Bound this issue to investigation and a decision, then identify implementation follow-ups and their owning repositories; reconcile any reopened `bgs-3cz` scope to avoid duplicate ownership.

- **MAJOR — 03: Reproduction can erase evidence without resolving historical questions.** The acceptance criteria suggest uninstalling the affected cache before establishing which process populated it or why its SHA became stale. Preserve the cache and metadata, require an isolated reinstall first, and allow an evidence-backed “historical cause undetermined” outcome because current reproduction cannot necessarily explain the June installation.

- **MAJOR — 03: Regression coverage could duplicate an existing passing test.** [`test_bundle_dir_stages_slim_plugin_payload`](/home/ketan/project/bugshot/tests/test_build_plugin.py:294) already verifies that staging excludes `node_modules`, `.beads`, `tests`, and `package.json`. Require coverage of the actual publication/install route and a functional installed-plugin smoke check; another staging assertion and a small directory size do not establish a working consumer fix.

- **MINOR — 02: The cross-repository dependency claim is false as written.** Installed `bd dep add --help` supports `external:<project>:<capability>` references resolved through configured external projects. Say direct issue-ID dependencies are unavailable or that cross-project integration is unconfigured, if verified; do not claim Beads cannot express cross-repository dependencies or imply priority necessarily propagates.

- **MINOR — 03: Public specifications are incorrectly grouped with unwanted payload.** [`scripts/build-plugin`](/home/ketan/project/bugshot/scripts/build-plugin:290) intentionally includes `docs/specs`, and INSTALL.md describes public specifications as shipped content. Replace the blanket “docs which no consumer needs” claim with the specific unnecessary directories, such as `docs/plans`.

- **MINOR — 05: The documentation check permits false positives.** The example searches directory basenames rather than exact `skills/<name>/SKILL.md` paths; existing AGENTS.md already mentions `vizdiff` outside its missing skill entry. Check exact relative paths with fixed-string matching and make missing entries produce a failing exit status.

The top fixes are to make 01’s migration and verification complete, separate 03’s investigation from publishing implementation while preserving evidence, and require consumer-install verification instead of repeating existing staging tests.
