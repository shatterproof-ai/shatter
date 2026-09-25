---
slug: 35vtk-9-swarm-config
kind: note-to-existing
title: "Note on str-35vtk.9: bento swarm no longer reads .claude/swarm-config.md; batch-landing claim in CLAUDE.md is unbacked"
priority: P3
type: task
labels: [agents, swarm]
parent_epic: "(existing issue; parent str-35vtk)"
blocked_by: []
existing_id: str-35vtk.9
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-35vtk.9: swarm config format changed

Target: **str-35vtk.9** (open, P1, "WS-F: Batch-landing protocol for swarm
leads (lands with WS-E)"). Action: `bd comments add str-35vtk.9` with the text
below. This note's own priority is P3, so do not change .9's priority. There
is no old draft for this note; it comes from finding agent-repo-19 (report §15.1).

## Comment text

> Audit 2026-09-22 (finding agent-repo-19, `audits/2026-09-22/findings.json`):
> this issue's premise is stale and its scope needs updating.
>
> - The body says "bento:swarm reads swarm-config.md" and plans a
>   `.claude/swarm-config.md` quality-gates rewrite. The installed bento
>   swarm no longer reads it:
>   `bento/plugins/claude/bento/skills/swarm/scripts/swarm-discover.py:18-22`
>   reads only `swarm-config.json` (repo root), `.claude/swarm-config.json` or
>   `.codex/swarm-config.json` (bento-96ua.2, closed). It validates a
>   `landing` block (`mode`, `gate_scope`, `full_gate`, `max_batch_size`,
>   `linger_minutes`, `batch_boundary_paths`; `:88-183`). An invalid or
>   absent block degrades to serial landing. The schema is in bento
>   `skills/swarm/references/landing-config.md`.
> - Shatter has only `.claude/swarm-config.md` (plus an untracked
>   `.codex/swarm-config.md` symlink in the primary). No `swarm-config.json`
>   exists, so swarms land serially and the .md gates (`/check-all`,
>   `/pre-completion`, `/walkthrough-review`) are at best read as prose.
>   `/check-all` also has `disable-model-invocation: true`
>   (`.claude/skills/check-all/SKILL.md:5`), so a lead cannot invoke it
>   through the Skill tool. Reference `task check` directly.
> - `CLAUDE.md:57` states "The lead runs one full `task check` at batch
>   landing", but nothing configured implements batch landing today.
>
> **Proposed scope update:**
> - Write the config at the repo root as `swarm-config.json`, so both
>   runtimes read it: `swarm-discover.py:18-22,257-270` checks the
>   runtime-specific file (`.claude/` or `.codex/swarm-config.json`) and then
>   the root file, and Codex never falls back to `.claude/swarm-config.json`.
>   Delete `.claude/swarm-config.md` and the primary's untracked
>   `.codex/swarm-config.md` symlink (or keep the `.md` only for Epic-mode
>   prose that bento does not read).
> - `landing` block: `full_gate: task check`, and `mode` / `max_batch_size`
>   matching this issue's protocol.
> - **`gate_scope` needs an adapter; `task affected` does not fit the
>   contract.** bento's `landing-config.md` defines `gate_scope` as a
>   "command that emits scoped gate commands for a diff", and teammates run
>   the emitted commands. `task affected` (`Taskfile.yml:511-514`)
>   *executes* the selected gates and prints status and logs; it emits no
>   commands. Add a small executable (for example `scripts/gate-scope.sh`)
>   that calls `python3 scripts/affected-gates.py --base <base> --head <head>`
>   and prints one runnable command per selected gate (`task <gate>`,
>   nothing for `(none)`), with no other stdout.
> - **Behavioural validation, not just warning-free discovery.** A
>   `gate_scope` string whose first word resolves on `PATH` passes
>   `swarm-discover.py` validation whatever it prints. Acceptance: (1)
>   `swarm-discover.py --runtime claude` and `--runtime codex` both report
>   the same non-null `landing` block with `mode` as configured and no
>   warnings; (2) a test runs the adapter on a fixture diff touching one
>   crate and asserts stdout is exactly the expected `task <gate>` lines,
>   and on a docs-only diff asserts it prints only the docs gates (or
>   nothing); (3) every emitted line runs successfully when executed as a
>   command in a scratch worktree (record the output in the close reason).
> - Re-check whether the manual batch-land skill this issue plans is still
>   needed, given bento swarm's built-in batch mode, and whether step (4)'s
>   "push once with --no-verify" survives. Audit decision D4 and the 08-24
>   note already rule out hook bypass, so drop it.
> - Until batch landing is configured and validated, soften `CLAUDE.md:57`
>   to say batch landing is planned (str-35vtk.9), not current.
