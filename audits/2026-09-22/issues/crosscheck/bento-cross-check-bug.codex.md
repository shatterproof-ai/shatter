# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 514cd9e62ab1d8cb1ac443f65cd647cfd3fb5ef96e044b53f5ca13d66ce7c150
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket bento-cross-check-bug (bento). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

- **MAJOR — Validation criteria would reject legitimate clean reviews.** The shared runner supports issue, plan, and code reviews; the code prompt explicitly permits finding nothing serious and asks whether changes are “safe to land.” Requiring severity-tagged findings or a “ready to file” verdict would reject valid clean code/plan reviews; define artifact-specific validation and test no-findings success cases.

- **MAJOR — Blanket hook bypass exceeds the demonstrated defect.** The evidence implicates `check-unpushed.py`, whereas the proposed exemption also disables PreToolUse guards against edits and destructive Git operations. Narrow the exemption to relevant lifecycle hooks, or justify each additional bypass; require regression checks that ordinary sessions still block correctly.

- **MAJOR — The neutral-directory workaround does not pass repository context through `--scope`.** In `run_cross`, scope is used when rendering the saved report but is never included in the counterpart’s prompt. Moving execution outside the repository while putting paths only in scope could prevent meaningful repository inspection; explicitly deliver repository context before treating this as a supported alternative.

- **MAJOR — Output preservation leaves the recoverable review ambiguous.** The draft distinguishes the original review from the hook reply captured in `last_file`, but does not specify which output must survive rejection. Require preservation of subprocess stdout, stderr, and the final-message file, with their paths reported; retaining only `last_file` would preserve the hook reply while still losing the original review.

- **MINOR — Reproduction does not identify the code actually under test.** The installed 2.3.73 runner, common helper, hook script, and hook registration differ from current source. Include the Codex version, complete invocation and fixture, source revision, and how rebuilt hooks reach the installed runtime; otherwise the proposed end-to-end check could exercise stale code.

The referenced false-success artifact exists and contains the quoted hook reply, and current source confirms the missing exemption and output deletion. I found no duplicate in the local tracker export, but its latest update is July 5, so current duplication remains unverified.

**Verdict: not ready to file as-is.** First narrow the hook exemption, make review validation compatible with all supported artifact types, and specify complete output preservation plus a reproducible runtime test.
