---
slug: global-guidance-actually-loads
kind: new
title: "Global Required-Loads guidance almost never loads: inject a short core-rules set into every Claude Code session"
priority: P1
type: bug
labels: [bug, documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Global Required-Loads guidance almost never loads: inject a short core-rules set into every Claude Code session

Part of #<epic>. Priority: P1. Type: bug.

The Codex half of this delivery is split out as `codex-render-composes-core-rules`, which is blocked by this issue because it needs the core file. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

The only auto-loaded global file is `codex/AGENTS.md`. `~/.claude/CLAUDE.md` imports it with `@../codex/AGENTS.md`, and Codex reads the rendered copy `~/.codex/AGENTS.md`. At line 16 it says "Follow the shared agent guidance in `~/dotfiles/docs/agent-guidance.md`". That line is a plain-text pointer, not an `@`-import. The index it points to (`docs/agent-guidance.md:8-16`) lists four "Required Loads" (branches-and-worktrees, read-before-designing, verification, and instruction-integrity, which is required "every session") plus about 15 conditional loads. All of them are prose pointers too.

The load-rate numbers below show that agents almost never read those files. P1 rests on that load rate alone: required rules that reach close to 0% of sessions are not in force. The audit also saw several failures that match the unloaded rules (skills documenting non-existent CLI commands, a stale plugin cache, agents offering gate bypasses), but the verifier noted that this causal link is inferred, not demonstrated.

## Evidence

Re-verified on 2026-09-23 against dotfiles `main` @ `81f35e1`.

- `codex/AGENTS.md:16` has a plain-text pointer to `~/dotfiles/docs/agent-guidance.md` and no `@`.
- `docs/agent-guidance.md:8-50` lists every leaf as a prose pointer, and none of them is imported.
- Load-rate scan of the Shatter project transcripts (`~/.claude/projects/-home-ketan-project-shatter/*.jsonl` and `*/subagents/*.jsonl`). It counts tool_use inputs whose `file_path` or command references `agent-guidance/` or `code-writing-guidance`:
  - Top-level sessions: 0/87 with the narrow pattern, 4/87 with a broad pattern that also counts the index file and Bash mentions.
  - Subagent transcripts: 1/166 (narrow) and 6/166 (broad).
- `wc -w codex/AGENTS.md` returns 1146.
- The hook-provenance sentence at `codex/AGENTS.md:23` says the worktree hook is "registered from `~/project/bento`". In fact `readlink -f ~/.claude/hooks/bento/require-worktree.sh` resolves to `~/.claude/plugins/cache/bento/bento/2.3.84/hooks/scripts/require-worktree.sh`, the installed plugin cache.

## Related open work (coordinate; do not duplicate)

- **dotfiles#18** puts a Definition of Done in `codex/AGENTS.md`, including the wiring-and-consumption rule and a pointer to drift-checks. The core file must not restate it.
- **dotfiles#20** converts guidance cross-references to absolute `~/dotfiles/docs/...` paths. That work is owned there.
- **dotfiles#23** caps `codex/AGENTS.md` at 800 words. This issue does not grow that file; the core file is injected separately.
- **dotfiles#21** covers the pending Codex render after guidance changes (relevant to `codex-render-composes-core-rules`).
- Closed #4 (Claude JSONL session lint) was closed as overscoped on 2026-09-07 and never built. No `claude/session-lint.py` exists. Do not assume that tooling.

## Acceptance criteria

- [ ] A core-rules file `~/dotfiles/docs/agent-guidance/core.md` exists, at most ~450 words, starting with a fixed marker line (for example `<!-- core-rules v1 -->`). It has one heading per rule, one or two imperative lines each, and a link to the leaf.
- [ ] Required Loads keep their force. For each of the four current Required Loads (branches-and-worktrees, read-before-designing, verification, instruction-integrity), `core.md` either states the leaf's essential constraints, or `docs/agent-guidance.md` keeps that leaf listed as a required load with its trigger. No Required Load becomes "optional depth" unless its essential constraints are in `core.md`.
- [ ] `core.md` also covers drift-checks, failing-checks and cross-repository boundaries. It must not duplicate #18's Definition of Done.
- [ ] Ownership of leaf summaries: `core.md` includes a one-line summary for each of these leaf rules that is on `main` when this issue lands: `never-recommend-bypass`, `background-wait-rule-and-hook`, `memory-lifecycle-rule`, `blocked-escalation`. Leaf rules that land later add their own line (each of those issues says so).
- [ ] Every Claude Code session receives `core.md` with no action by the agent. The mechanism is a SessionStart hook in `claude/settings.json` that prints the file, using a `$HOME/dotfiles/...` path, not `$DOTFILES` (see `hooks-dotfiles-env-unset`).
- [ ] Subagents: the closing comment states whether subagent sessions receive the injected context. If SessionStart does not reach subagents, either use a harness event that can add context to subagents, or record the gap explicitly in `docs/agent-guidance.md`.
- [ ] The hook-provenance note in `codex/AGENTS.md` (in whatever form survives #23) names the installed bento plugin cache instead of `~/project/bento`.
- [ ] Test: `claude/tests/test_core_rules_hook.py` extracts the SessionStart command from `claude/settings.json`, runs it under `env -i HOME=<tmp> PATH=/usr/bin:/bin` with `<tmp>/dotfiles` a copy of the repo, and asserts that stdout contains the marker line and every `##` heading of `core.md`. Run it with `python3 -m pytest claude/tests/test_core_rules_hook.py -q`. It must fail on the current tree (no such hook) and pass after the change.
- [ ] Delivery measurement, not read-call counts: over all top-level Claude Code sessions in `~/.claude/projects/*/` started in the 7 days after deployment, count the transcripts that contain the core marker line in hook-injected context. Success means 100% of top-level sessions. Report the subagent rate separately.

## Proof at close

The closing comment includes the red and green runs of the test, the delivery-measurement counts (numerator and denominator, and the script used), and an excerpt from one new session transcript that shows the injected marker.

## Suggested approach

1. Write `core.md` by condensing the leaves.
2. Add a SessionStart hook entry such as `cat "$HOME/dotfiles/docs/agent-guidance/core.md"`. Output from a SessionStart hook is added to context.
3. Update the "Required Loads" section of `docs/agent-guidance.md` to match the coverage criterion above.
4. Use a throwaway scan script for the measurement. Do not build the #4 toolchain.

## Out of scope

- Delivering the core rules to Codex (`codex-render-composes-core-rules`).
- Rewriting the leaf content.
- Absolute paths (#20), the Definition of Done (#18), the word budget itself (#23) and Codex render automation (#21).

## Dependencies

None blocking. Blocks `codex-render-composes-core-rules`.

## Source

Shatter audit 2026-09-22, findings plugins-06 (P1) and plugins-19 (P3; its path part is now owned by #20). The evidence is in `audits/2026-09-22/areas/plugins-guidance.md` on the shatter `audit-2026-09-22` branch.
