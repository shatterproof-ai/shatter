---
slug: u394l-4-skill-command-lint
kind: note-to-existing
title: "Note on str-u394l.4: skill-command lint requirements from the 2026-09-22 audit (subcommand resolution, not --help exit codes; governed suite runs; Taskfile parsing without task --list-all)"
priority: P2
type: task
labels: [agents, skills, lint]
parent_epic: "(existing issue; parent str-u394l)"
blocked_by: []
existing_id: str-u394l.4
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-u394l.4: skill-command lint requirements

Target: **str-u394l.4** (open, P2, "Agent rules drift lint"). Action:
`bd comments add str-u394l.4` with the text below. This transfers the lint
half of the earlier audit draft `repo-skills-rot` to its existing owner, so
there is one implementation; `repo-skills-rot` now only fixes the skills.

## Comment text

> Audit 2026-09-22 (finding agent-repo-14; `audits/2026-09-22/findings.json`).
> This issue stays the single owner of the agent-rules / skill-command lint.
> The audit adds these requirements and corrects two of the checks already
> listed in this issue's notes.
>
> **1. Resolve subcommands; do not trust `--help` exit codes.** Check (f)
> ("every `bd <verb> --flag` ... is accepted by `bd <verb> --help`") and this
> issue's own regression case would both miss `bd epic list`: on bd 1.1.0
> `bd epic list --help` **exits 0** and prints the parent `bd epic` help,
> whose "Available Commands" are only `close-eligible` and `status`.
> Validate a documented `bd a b c ...` by walking the command tree: at each
> level, the next word must appear in that level's "Available Commands"
> list (parse `bd <prefix> --help`), then check flags against the leaf's
> help. Same approach for any other CLI the lint covers. Regression tests:
> `bd epic list` FAILs, `bd epic status` and `bd update <id> --claim` PASS.
> Skip with a warning when `bd` is absent.
>
> **2. Cover skill bodies, and check governance, not just existence.**
> Scan `.claude/skills/**/SKILL.md` as well as CLAUDE.md/AGENTS.md. Fail on a
> bare suite command (`cargo test`, `go test ./...`, `npm test`, `jest`)
> **and** on an unwrapped namespace test task (`task core:test`,
> `task go:test`, ...): these tasks run cargo/go/npm directly and skip
> `scripts/gate-wrapper.sh`, the heavyweight-slot governance of
> str-35vtk.5. Accepted forms: governed gates (`task affected`, `task check`,
> `task parity`, `task conformance`, `task e2e*`, or
> `bash scripts/gate-wrapper.sh <label> task <ns>:test`), and targeted single
> tests preceded by a documented allowlist comment (define its exact form
> here; `repo-skills-rot` uses it in the bugfix skill). Derive the governed
> set from the Taskfiles (a task counts as governed if its `cmds` invoke
> `gate-wrapper.sh`), not from a hard-coded list.
>
> **3. Resolve `task <x>` names by parsing Taskfile YAML** (root plus
> `includes:` namespaces), **not** `task --list-all` / `--json`, which writes
> checksum state (see str-qwua7.3). This amends check (a) in the notes.
>
> **4. Close-time proof:** the lint run on the pre-fix tree (before
> <repo-skills-rot> lands) FAILs and lists at least audit `SKILL.md:115`
> (`bd epic list`), audit `:17-22` and bugfix `:65-70` (bare suites), and the
> check-go/check-rust/check-ts skills; after the skill fixes it PASSes. Record
> both outputs. The lint is wired into `task meta` `cmds:` and `sources:`
> and into drift-patrol, as this issue already requires.
