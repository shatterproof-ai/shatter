---
slug: global-guidance-actually-loads
kind: new
title: "Global Required-Loads guidance almost never loads: deliver a short core-rules set to every Claude and Codex session"
priority: P1
type: bug
labels: [bug, documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Global Required-Loads guidance almost never loads: deliver a short core-rules set to every Claude and Codex session

Part of #<epic>. Priority: P1. Type: bug.

## Problem

The only auto-loaded global file is `codex/AGENTS.md`. `~/.claude/CLAUDE.md` imports it with `@../codex/AGENTS.md`, and Codex reads the rendered copy `~/.codex/AGENTS.md`. At line 16 it says "Follow the shared agent guidance in `~/dotfiles/docs/agent-guidance.md`". That line is a plain-text pointer, not an `@`-import. The index it points to lists four "Required Loads" (branches-and-worktrees, read-before-designing, verification, and instruction-integrity, which is required "every session") plus about 15 conditional loads. All of them are prose pointers too, and agents almost never follow them.

The unloaded rules include drift-checks, failing-checks, instruction-integrity and cross-repository-boundaries. Their absence lines up with several 2026-09-22 Shatter audit findings: plugin skills that document CLI commands which do not exist, a plugin cache three months stale, and agents offering gate bypasses. The verifier notes that the causal link is inferred, not demonstrated.

## Evidence

Re-verified on 2026-09-23 against dotfiles `main` @ `81f35e1`.

- `codex/AGENTS.md:16` has a plain-text pointer to `~/dotfiles/docs/agent-guidance.md` and no `@`.
- `docs/agent-guidance.md:8-50` lists every leaf as a prose pointer, and none of them is imported.
- Load-rate scan of the Shatter project transcripts (`~/.claude/projects/-home-ketan-project-shatter/*.jsonl` and `*/subagents/*.jsonl`). It counts tool_use inputs whose `file_path` or command references `agent-guidance/` or `code-writing-guidance`:
  - Top-level sessions: 0/87 with the narrow pattern, 4/87 with a broad pattern that also counts the index file and Bash mentions.
  - Subagent transcripts: 1/166 (narrow) and 6/166 (broad).
  - Either way the load rate is close to zero.
- `wc -w codex/AGENTS.md` returns 1146.
- The hook-provenance sentence at `codex/AGENTS.md:23` says the worktree hook is "registered from `~/project/bento`". In fact `readlink -f ~/.claude/hooks/bento/require-worktree.sh` resolves to `~/.claude/plugins/cache/bento/bento/2.3.84/hooks/scripts/require-worktree.sh`, which is the installed plugin cache.

## Related open work (coordinate; do not duplicate)

- **dotfiles#18** puts a Definition of Done in `codex/AGENTS.md`. It already covers the wiring-and-consumption rule and a pointer to drift-checks. This issue must not restate those.
- **dotfiles#20** converts guidance cross-references to absolute `~/dotfiles/docs/...` paths. That work is owned there and is **removed from this issue**. It had been part of this issue's source finding, plugins-19.
- **dotfiles#23** caps `codex/AGENTS.md` at 800 words (Branches And Worktrees ≤ 30 words, Self-Improvement Loop ≤ 12 words). Inlining 6–8 rules into `codex/AGENTS.md` would break that budget, so the delivery mechanism below avoids growing that file.
- **dotfiles#21** covers the pending Codex render after guidance changes. Any change that must reach Codex depends on a render.
- Closed #4 (Claude JSONL session lint) was **closed as overscoped on 2026-09-07 and never built**. No `claude/session-lint.py` exists. Do not assume that tooling.

## Acceptance criteria

- [ ] A single core-rules file exists, for example `~/dotfiles/docs/agent-guidance/core.md` with at most ~350 words. It holds one or two lines per rule, each linking its leaf. At minimum it covers verification essentials, drift-checks, instruction-integrity, failing-checks (including the bypass-framing rule from `never-recommend-bypass`), cross-repository boundaries, and waiting for background work (from `background-wait-rule-and-hook`). The set must not duplicate the Definition of Done from #18.
- [ ] Every Claude Code session receives the core file with no action by the agent. The recommended way is a SessionStart hook in `claude/settings.json` that prints it as additional context, using a `$HOME/dotfiles/...` path and not `$DOTFILES` (see `hooks-dotfiles-env-unset`). If the maintainer prefers to inline the rules in `codex/AGENTS.md`, they must fit #23's 800-word budget.
- [ ] The issue states how Codex sessions receive the same rules, for example by appending the file during `codex/agents-sync.sh render`. After the change, `codex/agents-sync.sh status` reports `render vs snapshot: ok`.
- [ ] The hook-provenance note in `codex/AGENTS.md` (in whatever form survives #23) names the installed bento plugin cache instead of `~/project/bento`.
- [ ] Proof at close: a test under `claude/tests/` runs the SessionStart hook command and asserts that its output contains every core-rule heading. The closing comment includes the load-rate scan (the same patterns as above) re-run over sessions started after the change, and the transcript of one new session showing the injected context.

## Suggested approach

1. Write `core.md` by condensing the leaves. Keep each bullet imperative and short, and link the leaf for depth.
2. Add a SessionStart hook entry such as `cat "$HOME/dotfiles/docs/agent-guidance/core.md"`. Output from a SessionStart hook is added to context.
3. Change `docs/agent-guidance.md` so its "Required Loads" section says the core set is already loaded and the leaves are optional depth.
4. Measure again after about a week of sessions, using a throwaway scan script. Do not build the #4 toolchain.

## Out of scope

- Rewriting the leaf content.
- Absolute paths (#20), the Definition of Done (#18), the word budget itself (#23) and Codex render automation (#21).

## Dependencies

None blocking. `never-recommend-bypass`, `background-wait-rule-and-hook`, `blocked-escalation` and `memory-lifecycle-rule` each write a leaf rule, and this issue's core file summarises them. Whichever lands second adds the one-line summary.

## Source

Shatter audit 2026-09-22, findings plugins-06 (P1) and plugins-19 (P3; its path part is now owned by #20). The evidence is in `audits/2026-09-22/areas/plugins-guidance.md` on the shatter `audit-2026-09-22` branch.
