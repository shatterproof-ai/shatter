---
slug: never-recommend-bypass
kind: new
title: "Agents mark hook or gate bypass as the (Recommended) option: add a framing rule to failing-checks.md"
priority: P2
type: enhancement
labels: [documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Agents mark hook or gate bypass as the (Recommended) option: add a framing rule to failing-checks.md

Part of #<epic>. Priority: P2. Type: enhancement.

## Problem

Sometimes a gate or hook fails for a reason unrelated to the diff. In those cases agents offer a bypass (`--no-verify`, `core.hooksPath=/dev/null`, "waive this gate") as the **(Recommended)** AskUserQuestion option. Every recorded time, the user declined and asked for the flake to be fixed or isolated instead.

`failing-checks.md` already forbids self-granted waivers ("Never grant yourself a waiver … let the user decide"). It does not say how to frame the options, so agents obey the letter and steer the user toward the bypass.

## Evidence

Verbatim quotes from Shatter transcripts in `~/.claude/projects/-home-ketan-project-shatter/`, confirmed by the verifier:

- `87606e10`, 2026-09-19T22:39: the agent offered "Bypass this push's hook with --no-verify (Recommended)". The user chose "Keep retrying the push".
- `87606e10`, 2026-09-19T15:58: the agent offered a bypass. The user answered: "the tests shouldn't be using a globally visible directory like that. file an issue to get filesystem isolation then fix it."
- `e724dbd8`, 2026-09-07T23:09: the agent offered "File the bug now, proceed with --no-verify landings (Recommended)".
- `9f13ca23`, 2026-09-19T16:04: the agent proposed "Waive this gate and land". The user chose "Hold off, fix the flake first".

The rule's current text, re-verified at dotfiles @ `81f35e1`: `docs/agent-guidance/failing-checks.md` has 26 lines. `grep -in "recommended\|bypass\|no-verify"` on it finds nothing. The waiver bullet is the last bullet.

## Acceptance criteria

- [ ] `docs/agent-guidance/failing-checks.md` states the rule. When a gate or hook fails for a reason unrelated to the diff, the recommended option is to isolate or fix the cause, or to file it and pause. A bypass (`--no-verify`, `core.hooksPath` override, gate waiver, skip env var) may be listed only as a non-default option. It is never marked Recommended, never listed first, and never presented as the default.
- [ ] The user's "filesystem isolation" answer is quoted as the canonical example.
- [ ] The rule's one-line summary appears in the core-rules file from `global-guidance-actually-loads`. If that issue has not landed yet, it picks the line up.
- [ ] Proof at close: the closing comment shows the output of `grep -n "Recommended" ~/dotfiles/docs/agent-guidance/failing-checks.md` and of `codex/agents-sync.sh status`, with `render vs snapshot: ok` so Codex has the rule.

## Out of scope

- Enforcing this at the hook level (blocking `--no-verify`). That is tracked in bento as `git-guard-bypasses-and-false-positives`.
- Any guidance that recommends bypassing a hook. Per maintainer decision D4 (2026-09-23), no bypass guidance is written anywhere. Slow hooks are fixed at their root cause instead. For example, the shatter beads hook stall is fixed by retiring the JSONL import.

## Dependencies

None blocking. Related: `global-guidance-actually-loads`.

## Source

Shatter audit 2026-09-22 finding sessions-06.
