---
slug: repo-skills-rot
kind: new
title: "Repair rotted repo skills: delete check-go/rust/ts and protocol-sync, move audit and bugfix suite runs onto governed gates, fix audit skill paths/steps and add its memory-contradiction check"
priority: P2
type: task
labels: [agents, skills, docs, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [beads-retire-jsonl-import-dolt-remote]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Repair rotted repo skills: delete check-go/rust/ts and protocol-sync, move audit and bugfix suite runs onto governed gates, fix audit skill paths/steps and add its memory-contradiction check

## Problem

Several repo skills under `.claude/skills/` tell agents to do things the
project forbids, or things that no longer work:

- `check-go`, `check-rust` and `check-ts` run bare `go test ./...`,
  `cargo test` and `npm test`. Nothing references these three skills.
- `protocol-sync` hand-compares three protocol files. It ignores
  `protocol/registry.yaml`, the generated bindings and shatter-rust, all of
  which `task parity` and `scripts/protocol-codegen.py --check` already check
  mechanically.
- The `audit` skill runs bare suites in Phase 1, names a root `GLOSSARY.md`
  that does not exist, uses the invalid `bd epic list`, samples only the last
  20 commits in Phase 7, and in its post-audit step defers beads changes to
  `bd sync`, a command that no longer exists in bd 1.1.0. Under maintainer
  decision D4 (2026-09-23) the JSONL import and `bd sync` are retired and
  tracker sync moves to a Dolt remote. Phase 7 also points at the wrong
  memory path and never checks memory against repo facts. Stale project
  memory that told agents to bypass hooks with `--no-verify` /
  `core.hooksPath=/dev/null` went undetected through the 2026-09-04 audit.
- The `bugfix` skill runs bare module suites for its regression step.

**Why the fix is not simply "use `task <ns>:test`".** The project's
machine-wide heavyweight-slot governance (str-35vtk.5) lives in
`scripts/gate-wrapper.sh`, and only some tasks call it. `task core:test`
(`shatter-core/Taskfile.yml:14-30`) runs `cargo nextest` / `cargo test`
directly, so invoking it alone skips the slot semaphore, nice/ionice and the
timing CSV exactly as a bare `cargo test` does. Suite runs in skills must use
a **governed** invocation.

The skill-content lint that would catch this class of rot is owned by
str-u394l.4; this audit's requirements for it are in the note
`u394l-4-skill-command-lint`, not here.

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
- `.claude/skills/audit/SKILL.md`: `:17-22` bare `cargo test`, `cargo clippy`,
  `npm test`, `go test ./...`; `:72` lists `GLOSSARY.md` (only
  `docs/GLOSSARY.md` exists); `:115` `bd epic list` (bd 1.1.0 `bd epic` has
  only `close-eligible` and `status`); `:151` "Memory files in
  `.claude/projects/*/memory/`" (the real location is
  `~/.claude/projects/-home-ketan-project-shatter/memory/`); `:153`
  "Recent git log (last 20 commits)"; `:385` "Do NOT commit beads issue
  changes — those are handled by `bd sync`".
- `.claude/skills/bugfix/SKILL.md:26-31` and `:60-62` (single-test red/green
  commands), `:65-70` (module suites: `cargo test`, `cd shatter-ts && npm test`,
  `cd shatter-go && go test ./...`, `cd shatter-rust && cargo test`).
- Governed entry points: `task affected` (`Taskfile.yml:511-514`,
  `bash scripts/gate-wrapper.sh affected task affected-governed`),
  `task check`, `task parity`, `task conformance`, `task e2e*`. Namespace
  test tasks (`core:test`, `go:test`, `ts:test`, ...) are **not** wrapped.
  `gate-wrapper.sh` appends a row per governed run to
  `~/.cache/shatter/gate-times.csv` (`timestamp,worktree,label,...`).
- `bd sync --help` on bd 1.1.0 -> `Error: unknown command "sync"`.
- Audit sources: agent-repo-14, plus the audit-skill part of sessions-03 /
  agent-repo-08 (drafts `shatter-agent/14`, `shatter-agent/07`).

## Acceptance criteria

1. `check-go`, `check-rust`, `check-ts` and `protocol-sync` are deleted
   (preferred: nothing references them). If any is kept instead, it invokes
   only a governed gate (`task parity`, `task affected`, or
   `bash scripts/gate-wrapper.sh <label> task <ns>:test`) and the reason is
   in the close reason.
2. Suite-level runs in the `audit` and `bugfix` skills use a governed
   invocation: `task affected` for "run the affected suites", `task check`
   for the full gate, or `bash scripts/gate-wrapper.sh <label> task <ns>:test`
   for one namespace. No skill prescribes an unwrapped `task <ns>:test` or a
   bare suite command for a suite-level run.
3. The single-test red/green commands in `bugfix` may stay targeted
   (`cargo test -p <crate> <name>`, `go test -run <Name> ./<pkg>`,
   `npx jest -t <pattern>`), each preceded by a one-line allowlist comment
   saying why a focused single test is exempt from gate governance. Use the
   comment form that str-u394l.4's lint accepts (see
   `u394l-4-skill-command-lint`).
4. Audit skill fixes: `:72` -> `docs/GLOSSARY.md`; `:115` -> `bd epic status`;
   Phase 7 covers commits since the previous audit's SHA (fallback: last
   ~150); `:151` names the real memory path.
5. The audit skill's Phase 7 gains a **memory-contradiction step**: grep the
   project memory dir for hook-bypass advice
   (`grep -rnE -- '--no-verify|hooksPath' <memory dir>`, where an explanatory
   "do not" mention is allowed) and for claims that contradict AGENTS.md or
   current repo state (for example `core.bare`, the installed `bd version`,
   commands AGENTS.md no longer names). The findings are listed in the report.
6. The audit skill's `:385` `bd sync` reference is replaced by the D4
   procedure that `beads-retire-jsonl-import-dolt-remote` records in AGENTS.md:
   do not hand-commit `.beads/` files; tracker state lives in the local Dolt
   DB and reaches other machines via the Dolt remote. No `bd sync`, no
   JSONL-export commit, no hook-timeout variable and no hook-bypass
   instruction remains. The rest of the post-audit landing restructure
   belongs to str-qwua7.22 (note `qwua7-22-audit-land-before-file-note`);
   whichever of the two lands second rebases onto the other.
7. **Close-time proof (all recorded in the close reason):**
   - `grep -rnE 'cargo test|go test|npm test|bd sync|bd epic list|task [a-z-]+:test' .claude/skills/`
     before the change (24 matches on 2026-09-23) and after. After the
     change no match remains in the deleted skills, in audit Phase 1
     (`:17-22`), at audit `:115`/`:385`, or in bugfix's module-suite step;
     every remaining match is listed in the close reason with its class:
     allowlisted single test (item 3, with its comment), prose mention (for
     example audit `:172`, optimize-tokens `:74`), or a command CLAUDE.md
     itself prescribes (frontend-parity `:86`, the E2E suites);
   - one run of each governed invocation the skills now name, with the
     matching `gate-times.csv` row (label and exit code) showing it went
     through `gate-wrapper.sh`;
   - `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Do the deletions first (smallest diff), then the audit and bugfix edits.
`.codex/skills` in the primary checkout is a symlink to `../.claude/skills/`,
so deletions also disappear there.

## Out of scope

- The skill-command lint itself (str-u394l.4; requirements in
  `u394l-4-skill-command-lint`).
- Making every namespace test task governed (a Taskfile change; if wanted,
  file separately).
- The audit skill's post-audit landing flow (report via launch-work/land-work
  before filing): str-qwua7.22 (note `qwua7-22-audit-land-before-file-note`).
- AGENTS.md and `.beads/PRIME.md` `bd sync` removal: `beads-jsonl-consumers-drop-bd-sync`.
- Editing the memory files themselves: corrected by the maintainer on 2026-09-23.
- The frontend-parity skill (separate audit issue).

## Dependencies

- Blocked by `beads-retire-jsonl-import-dolt-remote` (bucket
  shatter-tracker-and-beads), for acceptance item 6 only: the skill must cite
  the sync procedure that issue records. Items 1-5 can start at once.
- Related: str-u394l.4 (agent-rules drift lint), str-qwua7.22 (audit Phase-10
  rewrite), str-qwua7.26.
