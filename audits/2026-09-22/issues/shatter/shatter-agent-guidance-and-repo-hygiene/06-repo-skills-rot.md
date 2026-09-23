---
slug: repo-skills-rot
kind: new
title: "Repair rotted repo skills: check-go/rust/ts and bugfix bare commands, superseded protocol-sync, audit skill paths/steps and memory-contradiction check, plus a skill-command lint"
priority: P2
type: task
labels: [agents, skills, docs, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [beads-retire-jsonl-import-dolt-remote]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Repair rotted repo skills: check-go/rust/ts and bugfix bare commands, superseded protocol-sync, audit skill paths/steps and memory-contradiction check, plus a skill-command lint

## Problem

Several repo skills under `.claude/skills/` tell agents to do things the
project forbids, or things that no longer work, and nothing lints skill content
against the Taskfile:

- `check-go`, `check-rust` and `check-ts` run bare `go test ./...`,
  `cargo test` and `npm test`. CLAUDE.md and `check-all` require the `task`
  facade (bare commands skip the heavyweight-slot wrapper, gate caching and
  parallelism budgets). Nothing references these three skills.
- `protocol-sync` hand-compares three protocol files. It ignores
  `protocol/registry.yaml`, the generated bindings and shatter-rust, all of
  which `task parity` and `scripts/protocol-codegen.py --check` already check
  mechanically.
- The `audit` skill uses bare commands in Phase 1. It names a root
  `GLOSSARY.md` that does not exist, samples only the last 20 commits in
  Phase 7, and in its post-audit step defers beads changes to `bd sync`, a
  command that no longer exists in bd 1.1.0. Under maintainer decision D4
  (2026-09-23), the JSONL import and `bd sync` are retired and tracker sync
  moves to a Dolt remote. Phase 7 also points at the wrong memory path and
  never checks memory against repo facts. Stale project memory that told
  agents to bypass hooks with `--no-verify` / `core.hooksPath=/dev/null` went
  undetected through the 2026-09-04 audit.
- The `bugfix` skill uses bare test commands.

## Evidence

Re-verified 2026-09-23 in `/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22`:

- `.claude/skills/check-go/SKILL.md:10` "Run `go test ./...` in `shatter-go/`";
  `check-rust/SKILL.md:10` "Run `cargo test` in the workspace root";
  `check-ts/SKILL.md:10` "Run `npm test` in `shatter-ts/`". A repo-wide grep
  for `check-go|check-rust|check-ts|protocol-sync` outside the skills
  themselves and `audits/` finds no references.
- `.claude/skills/protocol-sync/SKILL.md:10-14` reads only
  `shatter-core/src/protocol.rs`, `shatter-ts/src/protocol.ts`,
  `shatter-go/protocol/types.go` and optional `protocol/schemas/`.
- `.claude/skills/audit/SKILL.md`: `:17-21` bare `cargo test`, `npm test`,
  `go test ./...`; `:72` lists `GLOSSARY.md` (only `docs/GLOSSARY.md` exists);
  `:151` "Memory files in `.claude/projects/*/memory/`" (the real location is
  `~/.claude/projects/-home-ketan-project-shatter/memory/`); `:153`
  "Recent git log (last 20 commits)"; `:385` "Do NOT commit beads issue
  changes — those are handled by `bd sync`".
- `.claude/skills/bugfix/SKILL.md:29-31`, `:60-62`, `:67-70`: bare
  `cargo test`, `npm test`, `go test`, `cd shatter-rust && cargo test`.
- Task equivalents exist: namespaces `core`, `cli`, `ts`, `go`, `rust-fe`
  (`Taskfile.yml:12-30`), each with a `test` target, plus root `parity`
  (`:245`) and `conformance` (`:225`).
- `bd sync --help` on bd 1.1.0 -> `Error: unknown command "sync"`.
- Audit sources: agent-repo-14, plus the audit-skill part of sessions-03 /
  agent-repo-08 (drafts `shatter-agent/14`, `shatter-agent/07`).

## Acceptance criteria

1. `check-go`, `check-rust`, `check-ts` and `protocol-sync` are deleted, or
   rewritten as thin wrappers over `task go:test`, `task core:test` /
   `task cli:test`, `task ts:test` and `task parity`. The choice is recorded
   in the close reason.
2. `audit` and `bugfix` skills use the task facade (`task <ns>:test`) for
   suite runs. For the single-test red/green loop in `bugfix`, no crate
   Taskfile accepts pass-through args today (`CLI_ARGS` appears in none of
   them). Either add `{{.CLI_ARGS}}` to the `test` tasks, or keep the
   targeted bare command with a one-line note saying why it is allowed
   there. The lint in item 6 must accept whichever form is chosen.
3. Audit skill fixes: `:72` -> `docs/GLOSSARY.md`; Phase 7 covers commits
   since the previous audit's SHA (fallback: last ~150); `:151` names the
   real memory path.
4. The audit skill's Phase 7 gains a **memory-contradiction step**: grep the
   project memory dir for hook-bypass advice
   (`grep -rnE -- '--no-verify|hooksPath' <memory dir>`, where an explanatory
   "do not" mention is allowed) and for claims that contradict AGENTS.md or
   current repo state (for example `core.bare`, the installed `bd version`,
   commands AGENTS.md no longer names). The findings are listed in the report.
5. The audit skill's `:385` `bd sync` reference is replaced by the D4
   procedure that `beads-retire-jsonl-import-dolt-remote` records in AGENTS.md.
   That means: do not hand-commit `.beads/` files; tracker state lives in the
   local Dolt DB and reaches other machines via the Dolt remote
   (`bd dolt push` / `bd dolt pull`). No `bd sync` and no JSONL-export commit
   remain. The rest of the post-audit landing restructure belongs to
   `publish-audit-reports`. Whichever of the two lands second rebases onto
   the other.
6. A meta test (folded into str-u394l.4 if that lands first) is wired into
   `task meta` `cmds:` and `sources:`. It fails when a `.claude/skills/**/SKILL.md`
   names a bare `cargo test` / `go test` / `npm test` / `jest` invocation
   where an equivalent task exists, or names `task <x>` for a target that
   is not defined. Resolve targets by parsing the Taskfile YAML (root plus
   `includes:`), **not** `task --list-all --json`, which writes checksums.
   It also fails on `bd <subcommand>` names that `bd <subcommand> --help`
   rejects (skip if `bd` is absent).
7. Failing-then-passing proof for the lint: run it on the current tree (fails,
   listing the skills above), then after the fixes (passes). Record both in
   the close reason. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Do the deletions first (smallest diff), then the audit and bugfix edits, then
the lint, so the lint lands green. Check for skill symlinks under `.codex/skills`
in the primary checkout (it links to `../.claude/skills/`), so deletions also
disappear there.

## Out of scope

- The audit skill's post-audit landing flow (report via launch-work/land-work
  before filing): `publish-audit-reports`.
- AGENTS.md and `.beads/PRIME.md` `bd sync` removal: `beads-jsonl-consumers-drop-bd-sync`.
- Editing the memory files themselves: corrected by the maintainer on 2026-09-23.
- The frontend-parity skill (separate audit issue).

## Dependencies

- Blocked by `beads-retire-jsonl-import-dolt-remote` (bucket
  shatter-tracker-and-beads), for acceptance item 5 only: the skill must cite
  the sync procedure that issue records. Items 1-4 and 6 can start at once.
- Related: str-u394l.4 (agent-rules drift lint), str-qwua7.22 (audit Phase-10
  rewrite), str-qwua7.26.
