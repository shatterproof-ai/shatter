---
slug: never-recommend-bypass
kind: new
title: "Agents offer hook or gate bypass as the (Recommended) option: forbid offering bypass options in failing-checks.md"
priority: P2
type: enhancement
labels: [documentation]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: []
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Agents offer hook or gate bypass as the (Recommended) option: forbid offering bypass options in failing-checks.md

Part of #<epic>. Priority: P2. Type: enhancement. Cross-references to other drafts use their slugs; the filer posts a slug-to-issue map on the epic.

## Problem

Sometimes a gate or hook fails for a reason unrelated to the diff. In those cases agents offer a bypass (`--no-verify`, `core.hooksPath=/dev/null`, "waive this gate") as the **(Recommended)** AskUserQuestion option. Every recorded time, the user declined and asked for the flake to be fixed or isolated instead.

`failing-checks.md` already forbids self-granted waivers ("Never grant yourself a waiver … let the user decide"). It says nothing about which options an agent may offer, so agents obey the letter and steer the user toward the bypass.

## Evidence

Verbatim quotes from Shatter transcripts in `~/.claude/projects/-home-ketan-project-shatter/`, confirmed by the verifier:

- `87606e10`, 2026-09-19T22:39: the agent offered "Bypass this push's hook with --no-verify (Recommended)". The user chose "Keep retrying the push".
- `87606e10`, 2026-09-19T15:58: the agent offered a bypass. The user answered: "the tests shouldn't be using a globally visible directory like that. file an issue to get filesystem isolation then fix it."
- `e724dbd8`, 2026-09-07T23:09: the agent offered "File the bug now, proceed with --no-verify landings (Recommended)".
- `9f13ca23`, 2026-09-19T16:04: the agent proposed "Waive this gate and land". The user chose "Hold off, fix the flake first".

Current text, re-verified at dotfiles @ `81f35e1`: `docs/agent-guidance/failing-checks.md` has 26 lines. `grep -in "recommended\|bypass\|no-verify"` on it finds nothing. The waiver bullet is the last bullet.

## Acceptance criteria

- [ ] `docs/agent-guidance/failing-checks.md` states the rule. When a gate or hook fails for a reason unrelated to the diff, the agent offers only these options: isolate or fix the cause; file it and pause; keep retrying with evidence. The agent never proposes a bypass (`--no-verify`, a `core.hooksPath` override, a skip environment variable, a gate waiver) as an option, recommended or not.
- [ ] The rule stays consistent with the existing waiver bullet: the agent presents the evidence and the user decides. The agent does not add a bypass to the choices it presents.
- [ ] The user's "filesystem isolation" answer is quoted as the canonical example.
- [ ] No text added by this issue describes how to bypass a hook, anywhere (D4).
- [ ] If `docs/agent-guidance/core.md` (from `global-guidance-actually-loads`) is on `main` when this lands, this issue adds the rule's one-line summary to it. If not, that issue adds it.

## Proof at close

The closing comment includes:

- the new bullet as it reads in `failing-checks.md`;
- `grep -n -i "no-verify\|hooksPath" ~/dotfiles/docs/agent-guidance/failing-checks.md`, where every match is inside the prohibition;
- if `core.md` exists, the rule's summary line in the output of the SessionStart core-rules hook (and in `~/.codex/AGENTS.md` once `codex-render-composes-core-rules` has landed). `codex/agents-sync.sh status` alone is not proof: it does not look at leaf files.

## Out of scope

- Enforcing this at the hook level (blocking `--no-verify`). That is drafted in bento as `git-guard-bypasses-and-false-positives`.
- Slow hooks are fixed at their root cause, not bypassed. For example, the shatter beads hook stall is fixed by retiring the JSONL import (D4).

## Maintainer decisions that apply

- D4 (2026-09-23): no timeout environment variable and no hook-bypass guidance anywhere.
- D6: nothing from this audit is filed by agents; the maintainer runs the filer.

## Dependencies

None blocking. Related: `global-guidance-actually-loads`, `blocked-escalation`.

## Source

Shatter audit 2026-09-22 finding sessions-06.
