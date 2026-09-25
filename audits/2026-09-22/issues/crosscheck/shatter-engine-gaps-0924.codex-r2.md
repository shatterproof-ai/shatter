# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 061148c0c0f107b5d3c0d6f9cfeb894b2a0259e9c8262f78f47056d427dca6d8
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-engine-gaps-0924 (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

- **MAJOR — The review checkout is unavailable here.** The supplied working directory contains only `.agent-mode.local`, `batch2.txt`, `logs/`, and `one.sh`; it has no Shatter source tree to verify the drafts’ file and symbol references. The artifact identifies a different checkout at `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`, which is outside the available workspace.

- **MAJOR — “unknown config keys” bundles separate behaviors.** The draft combines config-file key warnings, `--set` validation and suggestions, strict mode, and handling an accepted-but-ignored flag on `scan`/`run`. Those changes have distinct implementation and compatibility decisions; split the ignored-flag behavior into its own issue or make the core config-warning scope explicit and estimate the CLI behavior separately.

- **MAJOR — The unknown-key warning’s exact-once requirement is underspecified.** It asks for one warning per key per run “at every load site” while also requiring deduplication across per-function and LLM parsing, without defining whether duplicate occurrences in separate files or config and override sources count as one key. Define the deduplication identity and whether warnings are per occurrence, source, or distinct path.

- **MINOR — The doctor issue leaves command status ambiguous.** It says an invalid config makes doctor “exit non-zero” and tests a “failing result,” but does not specify the exact exit code or whether both YAML and JSON failures should affect it when both files are present. State the expected exit status and precedence/combined-error behavior.

**Verdict:** Not ready to file as-is. First, verify code claims against the intended checkout; then split or narrow the bundled `--set` behavior and define warning deduplication and doctor exit semantics.
