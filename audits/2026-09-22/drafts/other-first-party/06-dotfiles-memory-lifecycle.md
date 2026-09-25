# Add a memory lifecycle rule: tooling bugs go to the tracker; memory is a pointer that retires when the issue closes

## Filing metadata

- tracker/repo: dotfiles
- action: create new issue
- type: enhancement
- priority: P2
- labels: documentation, enhancement
- parent: repo epic (see INDEX)
- dedupe relation: partially-covered (dotfiles#5-#7 closed built audits, not the rule)
- source findings: plugins-15

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

Agent auto-memory stands in for a tracker, and entries go stale without anyone noticing. In the Shatter project memory (`~/.claude/projects/-home-ketan-project-shatter/memory/`):

- `project_shatter_gate_cache_and_bare_primary.md` says the primary checkout has `core.bare=true`. It is `false` (`git -C /home/ketan/project/shatter config --show-origin core.bare` -> `file:.git/config false`). The stale claim was repeated in the 2026-09-22 audit's own prompt.
- `project_shatter_git_hook_test_corrupts_worktree.md` still prescribes `--no-verify` after the underlying bug (shatter str-jttrf) was fixed. Agents quote it verbatim as justification for bypassing hooks.
- `project_audit_2026_07_10_gate_state.md` exists but is not indexed in `MEMORY.md`.

`codex/AGENTS.md` "Self-Improvement Loop" (around lines 57-60) says "update memory with the lesson". It has no issue-first rule and no retirement rule. Closed dotfiles #5-#7 built memory freshness, drift and duplicate audit tooling, but nothing requires memories to be retired.

## Acceptance criteria

- [ ] `codex/AGENTS.md` Self-Improvement Loop states: a tooling or product bug goes to the owning repo's tracker first. Memory holds a one-line pointer with the issue ID and date. When the issue closes, the memory is deleted or rewritten. Workaround memories (bypass instructions) must name the fixing issue.
- [ ] The guidance says whether project facts belong in global memory or in `bd remember`, and when to use each.
- [ ] The #5-#7 memory audit tooling flags memories that contain bypass instructions (`--no-verify`, `core.hooksPath`) or that reference a closed tracker issue.

## Out of scope

Cleaning up the specific Shatter memory files. That is tracked in the shatter repo.

## Source

Shatter audit 2026-09-22 findings plugins-15 and sessions-03.
