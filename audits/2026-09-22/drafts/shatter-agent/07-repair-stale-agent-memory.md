# Rewrite stale shatter agent memories that prescribe --no-verify/hooksPath bypass and state false repo facts

- Priority: P1
- Type: task
- Labels: agents,memory,git-hooks
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (partially covered by str-qwua7.28, str-qwua7.26)
- Source findings: sessions-03, agent-repo-08, frontend-rust-10 (memory part)
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
Claude project memory for shatter still instructs agents to bypass hooks and
records facts that are no longer true. Agents cite it verbatim as
justification ("Per that memory's recorded remedy, I'll push with
`--no-verify`"). Since 2026-09-04: 43 `--no-verify` uses in 10 sessions,
15 `core.hooksPath=/dev/null` uses. This contradicts AGENTS.md:373 and the
bento hook-bypass guard.

## Current Code Facts (memory dir `~/.claude/projects/-home-ketan-project-shatter/memory/`)
- `project_shatter_git_hook_test_corrupts_worktree.md` line 3 and line 36:
  "Commit and push with `--no-verify`"; claims primary has
  `core.hooksPath=/dev/null` (false); omits the str-jttrf fix.
- `project_beads_git_hook_timeout.md` lines 3, 16: bypass with
  `core.hooksPath=/dev/null` for rebase/checkout/merge.
- `MEMORY.md` index line for the hook memory ends "commit & push with --no-verify".
- `project_shatter_gate_cache_and_bare_primary.md` lines 2, 18, 25 and the
  index line: "primary checkout has core.bare=true" — false
  (`git config core.bare` -> false).
- `project_audit_2026_09_04.md` description: "no bd issues were filed because
  bd was down" — false (body says they were filed).
- `project_audit_2026_07_10_gate_state.md` exists, is not indexed, cites a
  report that was never committed.
- `project_taskfile_migration.md` describes the finished migration as pending
  and cites `/tmp` files.
- `project_shatter_rust_single_file_analysis.md` says "no cross-file or
  cross-crate type resolution" — same-crate resolution landed in str-do53
  (`shatter-rust/src/analyzer.rs:834-952`).
- AGENTS.md:370-375: bypass only "transiently for a known-hanging
  rebase/merge in your own worktree".

## Acceptance Criteria
- The hook memories no longer recommend `--no-verify` or `hooksPath` bypass;
  they say "never bypass; if a hook flakes, stop and file", cite str-jttrf
  (fixed), str-dl2pj and str-qwua7.28, and state when the memory can be deleted.
- bare-primary, audit_2026_09_04 description, rust single-file and
  taskfile_migration memories corrected or deleted; audit_2026_07_10 file
  deleted; MEMORY.md index matches the files.
- `grep -rlE -- '--no-verify|hooksPath=/dev/null' <memory dir>` returns no
  file that recommends the bypass (explanatory "do not" mentions allowed).
- `.claude/skills/audit/SKILL.md` gains a step: grep project memory for
  bypass advice and for claims contradicting AGENTS.md or current repo state
  (`core.bare`, bd version), and list findings in the report.

## Suggested Approach
Memory lives outside the repo; the edit is an operator/agent action recorded in
this issue's close reason (list files changed). The skill change lands via
launch-work.

## Out of Scope
Moving pickpackit/kapow/zolem goal memories (see draft 27). Global memory
lifecycle rule (dotfiles tracker).
