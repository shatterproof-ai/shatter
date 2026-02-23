# Agent Instructions

This project uses **bd** (beads) for issue tracking. Run `bd onboard` to get started.

## Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --status in_progress  # Claim work
bd close <id>         # Complete work
bd sync               # Sync with git
```

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd sync
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds

## Creating Issues

Use the correct issue type:
- `epic` — a grouping container for related features/tasks. Not directly workable.
- `feature` — delivers new user-visible or agent-visible capability
- `task` — scaffold, chore, or infrastructure work (e.g., project init, CI setup)
- `bug` — something broken that needs fixing

### Creating Epics

Epics must use `--waits-for-gate` so they are blocked until their children complete, and do not appear in `bd ready`:

```bash
bd create "Epic: Feature Area" -t epic -p 2 -l label1,label2 \
  -d "Description of the epic" \
  --waits-for-gate all-children
```

This makes the epic resolve automatically when all children are closed. Never leave an epic as a bare open issue with no gate — it will pollute `bd ready`.

### Creating Child Issues Under Epics

Use `--parent` to place issues under an epic, and `--waits-for` to make the epic wait for the child:

```bash
bd create "Implement feature X" -t feature -p 1 -l core,rust \
  --parent str-abc \
  --waits-for str-abc \
  -d "Description" \
  --acceptance "Acceptance criteria"
```

The `--parent` flag establishes the hierarchy. The `--waits-for str-abc` flag tells the epic to wait for this child before it can close.

### Dependencies Between Issues

Use `bd dep add` to express ordering constraints between issues:

```bash
bd dep add str-child str-dependency   # str-child is blocked by str-dependency
```

Only add dependencies where there is a real technical ordering constraint (e.g., "the executor cannot be built before the instrumentor exists"). Do not add dependencies for soft preferences or nice-to-have ordering.

### Completing an Issue

An issue is not complete until:
1. All code changes pass quality gates (tests, clippy/lint, build)
2. Changes are committed on a feature branch
3. The branch is merged to `main`
4. The branch is deleted after merge
5. The issue is closed with `bd close <id>`
6. Changes are pushed to remote with `bd sync && git push`

Do not leave stale branches. Merge and delete promptly.

## Git Workflow

- Work on feature branches, not `main` directly
- Branch names should reference the issue: `str-<hash>-short-description`
- Commits should be clean and atomic — one logical change per commit
- Rebase feature branches onto `main` before merging
- After merge, delete the feature branch both locally and remotely
