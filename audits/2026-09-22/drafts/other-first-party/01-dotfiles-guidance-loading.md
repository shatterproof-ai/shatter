# Make the global Required-Loads guidance actually load (inline core rules; absolute paths)

## Filing metadata

- tracker/repo: dotfiles
- action: create new issue
- type: bug
- priority: P1
- labels: bug, documentation
- parent: repo epic (see INDEX)
- dedupe relation: related (dotfiles#9, #10, #4 closed); treated as new
- source findings: plugins-06, plugins-19

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

`codex/AGENTS.md` is @-imported into every Claude session. It tells agents to "Follow the shared agent guidance in `~/dotfiles/docs/agent-guidance.md`", but that index is not @-imported. It lists "Required Loads" (branches-and-worktrees, read-before-designing, verification, instruction-integrity "every session") and about 15 conditional loads as prose pointers. Agents almost never read them. In the Shatter project transcripts (`~/.claude/projects/-home-ketan-project-shatter/*.jsonl`), a scan of tool_use inputs that referenced `agent-guidance/` or `code-writing-guidance` found reads in roughly 0–4 of 87 top-level sessions and 1–6 of 166 subagent transcripts, depending on the match pattern. The unloaded rules include wiring-and-consumption, drift-checks and failing-checks, which are exactly the rules whose absence produced several audit findings. Examples: plugin skills documenting CLI commands that do not exist, and a stale installed plugin cache.

Two smaller problems sit in the same files:

1. `docs/agent-guidance.md` and `docs/code-writing-guidance.md` list leaf files as repo-relative paths (`docs/agent-guidance/verification.md`). From a consumer repo's cwd (for example shatter, which has its own `docs/` with no `agent-guidance/`), these do not resolve. `docs/agent-guidance/instruction-integrity.md` says an unresolvable referenced rules file means the environment is broken.
2. `codex/AGENTS.md` (around line 23) says the worktree-enforcing hook is "registered from `~/project/bento`". It actually resolves from the installed plugin cache: `~/.claude/hooks/bento/require-worktree.sh` -> `~/.claude/plugins/cache/bento/bento/<ver>/hooks/scripts/require-worktree.sh`.

## Current code facts

- `/home/ketan/dotfiles/codex/AGENTS.md`: "Follow the shared agent guidance in `~/dotfiles/docs/agent-guidance.md`" (plain text, no `@`).
- `/home/ketan/dotfiles/docs/agent-guidance.md` lines ~11-26: repo-relative leaf paths.
- Prior closed issues #9 and #10 restructured routing into conditional leaf loads. #4 built a Claude JSONL session lint that could measure load rate.

## Acceptance criteria

- [ ] The truly global rules (at least: verification essentials, wiring-and-consumption, drift-checks, instruction-integrity, failing-checks, cross-repo boundaries) appear as short inline bullets in `codex/AGENTS.md`, or are injected by a SessionStart hook, so they reach every session with no action by the agent. Leaf files remain as optional depth.
- [ ] Every path in `docs/agent-guidance.md` and `docs/code-writing-guidance.md` is absolute (`~/dotfiles/docs/...`).
- [ ] The hook-provenance sentence in `codex/AGENTS.md` names the installed bento plugin cache.
- [ ] The #4 transcript lint (or a new small script) reports the guidance-load rate per project, so the conditional-load approach can be measured.

## Suggested approach

Pick about 8 rules that must always apply and inline each as a 1–2 line bullet with a link to its leaf file for depth. Measure again with the lint after a week of sessions.

## Out of scope

Rewriting the leaf content itself.

## Source

Shatter audit 2026-09-22 findings plugins-06 and plugins-19 (`audits/2026-09-22/areas/plugins-guidance.md` on the shatter `audit-2026-09-22` branch).
