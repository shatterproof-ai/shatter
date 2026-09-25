# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** ddf0a70d8e4048a04307e7f2dd13d86edc4287308fe93755156fd263f83889fa
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-int-width-signedness (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

- **MAJOR — Epic bundles independent work and an unrelated reporting fix.** The three integer-range changes form a dependency chain, but `rust-input-deserialize-classification` is explicitly standalone and defence in depth. Making the epic complete only when all four close ties reporting work to the integer representation project and broadens its scope.

- **MAJOR — The i128 core issue has conflicting range and value requirements.** It requires an exact `i128` range carrier, then clamps `u128` to `u64` and `i128` to `i64`; it also says “model extraction returns `i128`” while the suggested approach keeps `ConstValue::Int` as `i64`. Clarify which value paths are widened and what ranges the issue promises to represent.

- **MAJOR — The first child’s tests do not clearly prove width bounds.** Its property criterion says values are “within width” for every width, while the proposed clamp for unsigned widths ≥64 only guarantees nonnegative `i64` values; those conditions differ for widths 8–32 and signed types. State exact expected bounds per case and which entry points are checked.

- **MAJOR — Several acceptance criteria depend on unavailable or untracked artifacts.** The Rust reproduction refers to an external examples checkout and an untracked audit transcript, and requires pasting output from a main build that the draft says was not recorded. Commit the minimal fixture and provide a reproducible command and expected observable result; otherwise a fresh agent cannot reliably verify the baseline.

- **MAJOR — E2E criteria conflate generation correctness with error classification.** The first child requires zero “deserialization failed” rows, while the separate classification child changes how those failures are reported. Specify whether the integer fix must prevent invalid inputs regardless of classification, and make each child’s proof independent.

- **MINOR — The Go alias compatibility criterion assumes release coordination outside the issue.** It requires filing a follow-up and waiting for a continuous release to ship before closing, but gives no release process or clear close condition. Define the compatibility window and a verifiable release criterion, or track alias removal separately.

- **MINOR — Duplication and current status are not verifiable from the draft.** It names related issue IDs and claims the audit parent exists, but gives no tracker links; the “new epic” and children may duplicate open or closed work. Include canonical tracker links and confirm these drafts are not already filed.

**Verdict:** Not ready to file as-is. First, resolve the i128 issue’s representation and range contract; second, make the reproduction fixture and baseline runnable from the repository; third, separate the standalone deserialization-classification work from the integer epic and clarify each child’s independent acceptance criteria.
