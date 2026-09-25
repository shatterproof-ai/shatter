# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 0bec358009e868e5a7b7e3f3215d0bfc8d73c5ce37afda160bd9835b778f6591
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket bento-guards-doctor-tracker (bento). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

I checked relevant source at `b1bb787` and local CLI help. Tracker verification was blocked: `bd show` requires a database lock unavailable in this read-only sandbox, so ticket statuses and duplication remain unverified.

- **BLOCKER — #09’s deletion criteria do not establish that contents are disposable.** An unregistered directory with no process inside can still contain the only copy of uncommitted work; the doctor’s “safe to remove” wording is not evidence otherwise. Require content preservation or explicit review of potentially valuable files before deletion, with tests covering those cases.

- **MAJOR — #03’s warnings can remain invisible during normal landing.** `land.py:_run_script` captures child stderr and discards it on success, so adding a warning to `land-work-create-preview.py` does not ensure users see it. Require a test through `land.py` proving the warning reaches users while the slow operation is running; an after-completion warning leaves the unexplained wait unresolved.

- **MAJOR — #04 incorrectly requires remote synchronization between linked worktrees.** Local `bd worktree --help` explicitly states that worktrees share the main repository’s database through Git common-directory discovery. Distinguish shared worktrees from independent clones/databases when prescribing Dolt remotes and missing-remote warnings.

- **MAJOR — #05 lacks a workable session-ownership contract.** Ordinary reflogs do not contain agent session IDs, and a marker written only by `land.py` cannot identify an independently launched background `git push`. Specify ownership recording, session-ID propagation, and handling of concurrent landers, stale markers and reused PIDs.

- **MAJOR — #06 introduces shared writes without concurrency requirements.** `_rewrite_agent_mode_keys` performs an unlocked read/modify/write using a fixed `.agent-mode.local.tmp` filename. Moving every worktree onto that file requires concurrent-update tests and preservation of existing decisions; the proposed sequential test can pass while parallel sessions lose updates.

- **MAJOR — #08 moves previews without updating their discovery location.** The doctor defaults to scanning `/tmp`, while the acceptance criteria move preview creation to `TMPDIR`. Require creation and discovery to agree on locations, with a test proving a stale preview outside `/tmp` is still reported.

- **MAJOR — #10 and #11 overlap on an unspecified tracker-closing lifecycle.** At the cited revision, `land.py` has no close step and invokes verification without `--issue`; closure happens afterward in the skill workflow. Assign one issue ownership of introducing and wiring closure, define its ordering relative to landing verification, and add dependencies so neither ticket can pass through an unused helper or optional check.

- **MAJOR — #10/#12 leave the reported closure path unprotected.** #10 identifies closures outside `land.py` as the problem, but only mandates wiring the helper into `land.py`; direct `bd close` remains available without either reason validation or the open-children check. Specify how supported closure workflows adopt the helper and accurately limit any enforcement claim.

- **MINOR — #10’s reason rules contradict its accepted forms.** `wontfix: obsolete` follows an explicitly accepted form but fails the 20-character minimum, while sufficiently long arbitrary prose could pass the stated checks. Define validation by reason type and test representative valid reasons alongside invalid ones.

- **MAJOR — #14 assumes an executable report validator that does not exist.** The audit skill provides prose report instructions and a discovery script, without a component-grade schema or rejection mechanism. “The template rejects a grade” silently adds implementation scope; specify the validator and its caller or make this a review checklist requirement.

- **MAJOR — #14’s evidence alternatives still permit its motivating mistakes.** A direct behavioral probe can exercise unreachable code, while a production-caller citation cannot establish that timeouts are enabled by default. Require evidence appropriate to each claim: production reachability for integration claims and default-configuration probes for behavioral claims.

- **MAJOR — #04’s mandatory closing evidence depends on disappearing external state.** Requiring doctor output from shatter “before its migration” can become impossible once the independently tracked migration lands. Attach a preserved reproducer or allow equivalent fixture evidence, rather than making completion depend on another repository remaining broken.

**Verdict: not ready to file as-is.** Prioritize safe deletion criteria, a single explicit closure/session-ownership design, and acceptance tests that exercise the actual calling workflows. Reconcile related tracker issues before filing; duplication has not been independently cleared.
