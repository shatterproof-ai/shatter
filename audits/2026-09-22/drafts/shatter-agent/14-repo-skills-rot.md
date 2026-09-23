# Repair rotted repo skills (check-go/rust/ts bare commands, superseded protocol-sync, audit skill paths/steps)

- Priority: P2
- Type: task
- Labels: agents,skills,docs
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (partially covered by str-u394l.4, str-qwua7.22, str-qwua7.26)
- Source findings: agent-repo-14
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
Several repo skills tell agents to do things the project forbids or that no
longer work, and nothing lints skill content against the Taskfile.

## Current Code Facts
- `.claude/skills/check-go`, `check-rust`, `check-ts` SKILL.md run bare
  `go test ./...`, `cargo test`, `npm test`; CLAUDE.md and `check-all`
  forbid bare commands when a `task` target exists. Nothing references these
  three skills.
- `.claude/skills/protocol-sync/SKILL.md` compares three hand-written files
  and ignores `protocol/registry.yaml`, generated bindings and shatter-rust;
  `task parity` and `scripts/protocol-codegen.py --check` already cover this.
- `.claude/skills/audit/SKILL.md`: Phase 1 uses bare commands; line ~72 lists
  `GLOSSARY.md` (real path `docs/GLOSSARY.md`); Phase 7 reads only "last 20
  commits"; line 385 commits on main and defers to `bd sync`.
- `.claude/skills/bugfix/SKILL.md:29-31,60-70` use bare test commands.
- All 97 `task` references and all script/doc paths in agent docs resolve
  (so a lint is cheap).

## Acceptance Criteria
- check-go/check-rust/check-ts and protocol-sync deleted, or rewritten as thin
  wrappers over `task go:test`/`task core:test`/`task ts:test`/`task parity`.
- audit and bugfix skills use the task facade, correct paths, and Phase 7
  covers commits since the previous audit (or ~150).
- A meta test (fold into str-u394l.4 if it lands first) fails when a skill
  under `.claude/skills/` names a bare `cargo test`/`go test`/`npm test`/`jest`
  invocation where an equivalent task exists, or names a `task <x>` that is
  not in `task --list-all` (use YAML parsing, NOT `task --list-all --json`,
  which writes checksums).

## Out of Scope
Audit Phase-10 landing (draft 06). frontend-parity skill (draft 15).
