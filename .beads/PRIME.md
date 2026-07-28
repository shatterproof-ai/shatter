# Beads Context

Use **bd** for all task tracking — not TodoWrite or markdown files.
Create/claim the issue before writing code; mark it `in_progress` when starting.

## Session close

Before saying "done": `git status` → `git add <files>` → `bd dolt pull` →
`git commit` → `git push`. Merge to `main` only through the landing flow in
`AGENTS.md`.

## Core commands

- `bd ready` — issues ready to work (no blockers)
- `bd show <id>` — issue detail with dependencies
- `bd create --title=… --description=… --type=task|bug|feature --priority=2`
- `bd update <id> --claim` — claim work
- `bd close <id1> <id2> …` — close completed issues
- `bd dep add <issue> <depends-on>` — ordering constraint
- `bd remember "insight"` / `bd memories <keyword>` — persistent knowledge;
  run `bd memories <keyword>` when stored project memory is needed

Full command reference: `bd prime --full`. Shatter-specific extensions:
`AGENTS.md`.
