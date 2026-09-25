# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 2d6c0800a76c2ed0732a9f1bc389ef7b49f02660ab8753038df0e9b9b38fab10
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-agent-guidance-and-repo-hygiene (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

**Not ready to file as-is.** I checked repository code at `70465921` and installed plugin contracts. Live tracker access failed in this read-only environment; duplication findings below use the checked-in tracker export, whose current statuses remain unverified.

- **MAJOR — 01: Config snapshots cannot cover fixtures the test never executes.** The proposed hashes surround `ENTRYPOINTS`; a new fixture omitted from that list still never runs, so the draft’s stated missing-entrypoint scenario remains undetected. Require registration completeness or narrow the claimed protection.

- **MAJOR — 02: The skip rule conflicts with detecting `core.bare=true`.** “SKIP when not in a git work tree” includes the corrupted-checkout condition this check must report as FAIL. Specify repository discovery and config inspection before deciding to skip, and test that exact precedence.

- **MAJOR — 03: The identity correction retains the wrong environment variable.** Installed `bd 1.1.0 --help` identifies **`BEADS_ACTOR`**, not `BD_ACTOR`, as the actor override. Checking only `BD_ACTOR` cannot establish the claimed fallback; distinguish actor, assignee and owner, and verify the effective actor before dropping the identity requirement.

- **MAJOR — 04: The proposed ancestry rule forbids valid diagnosis of feature branches.** Requiring every diagnosis or close-reason SHA to be an ancestor of `origin/main` prevents citing an unmerged regression reproduction or fix. Restrict ancestry verification to claims that work is landed or that a tested revision represents main; otherwise require an accurately labelled tested SHA.

- **MAJOR — 04/05/07: Cleanup acceptance has no outcome when deletion approval is declined.** These issues require explicit confirmation yet also require the branches/directories to disappear, leaving completion undefined if preservation is requested. Separate cleanup from the independently useful incident, gitignore and configuration changes, or define a documented retained/deferred outcome.

- **MAJOR — 06: The proposed task replacements do not restore heavyweight-slot governance.** For example, [`shatter-core/Taskfile.yml`](/home/ketan/project/shatter/shatter-core/Taskfile.yml:14) runs Cargo directly under `core:test`; invoking that task alone does not acquire the wrapper used by `task affected`. Specify a governed invocation and verify wrapper execution instead of treating every task target as governed.

- **MAJOR — 06: The command-lint oracle accepts an invalid command.** I verified that `bd epic list --help` exits **0** and prints parent help, although `list` is absent from the available epic commands. The proposed help-rejection check therefore misses the existing lint issue’s explicit regression case; validate command resolution, including nested subcommands.

- **MAJOR — 06/07/10: Existing ownership is acknowledged but not reconciled.** The tracker export already assigns command linting to `str-u394l.4`, plugin decisions/wiring to `.52/.53`, and status banners to `.44`; “folded in if it lands first” permits duplicate implementations. Assign one owner per deliverable and make the new drafts explicit extensions, dependencies or transfers.

- **MAJOR — 09: Story-impact review is placed after the change it must govern.** The installed skill requires execution **before behavioral edits**, gated by `docs/stories/INDEX.md`; adding it only to completion guidance risks discovering protected intent after implementation. Put invocation in the pre-edit workflow and have completion verify that it happened.

- **MAJOR — 09: Touching SPEC is insufficient proof of the required documentation updates.** Acceptance item 2 passes whenever `SPEC.md` changes, even if the change omits the matching section, changelog row and date required by item 1; exit-code changes can also occur in `main.rs` without touching `args.rs`. Define the affected paths and require a semantic checklist alongside the file-presence heuristic.

- **MAJOR — 11: `gate_scope: task affected` violates the swarm output contract.** Swarm expects a selector emitting gate commands, whereas [`task affected`](/home/ketan/project/shatter/Taskfile.yml:511) executes gates and emits status/log output. Require a selector adapter and behavioral output validation; warning-free discovery alone does not prove compatibility.

- **MINOR — 11: Claude-only JSON leaves Codex unconfigured.** Installed discovery checks `.codex/swarm-config.json` and root `swarm-config.json` for Codex, without falling back to `.claude/swarm-config.json`. Use shared root configuration or explicitly scope and test both runtimes.

- **MINOR — 04: The incident description reverses the agent-directory change.** `git show --name-status e50fc399 -- .claude/agents` reports deletion of all three agent files, not restoration. Correct the statement or identify the different comparison baseline being described.

The top fixes are to correct the executable contracts and contradictory acceptance rules, reconcile ownership with existing issues before filing, and separate optional cleanup from independently completable changes.
