# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 9c97ab8524c686760027c7333798b540a6c8745836d854d1d51336c6d764a389
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-int-width-signedness (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

- **MAJOR — The epic’s child relationships contradict each other.** It says `go-uint-alias-removal` is a child and says each child names the epic via `parent_slug`, but the alias-removal draft sets `parent_epic` to the audit epic. The epic also says this follow-up cannot hold the epic open, while listing only three issues in its closure condition; correct the parent metadata and define whether the follow-up is in or out of the epic.

- **MAJOR — The Go issue bundles a migration across several independently testable surfaces.** It combines analyzer mapping, core alias compatibility, planner and handler behavior, export, generated bindings, parity/conformance, and end-to-end bounds checks. Split the protocol migration from retiring or adapting the Go-specific planner and handler paths, or narrow the issue around one owner-visible result; the current M estimate and single proof obscure the scope.

- **MINOR — A superseded Rust draft remains in the audit materials.** The references say this bundle supersedes `audits/2026-09-22/drafts/shatter-code/82-rust-usize-negative-inputs.md`, which is still present and describes the same negative-`usize` symptom. Clarify that the old draft is unfiled and retired, or remove its active-index entry, so a future filer does not create duplicate work.

**Verdict:** Not ready to file as-is. Fix the epic/parent relationship, split or narrow the Go migration scope, and mark the older Rust draft as retired. The detailed fixtures, reproduction commands, and acceptance checks make the individual technical goals unusually actionable, but they do not resolve those filing ambiguities.
