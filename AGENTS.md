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

## Issue Title Guidelines

Titles appear in `bd list`, `bd ready`, and terminal window titles where space
is limited. The **first ~20 characters** must be meaningfully descriptive:

| Bad | Good |
|---|---|
| `Add support for symbolic integers` | `Symbolic int support` |
| `Implement the protocol message for branch data` | `Branch data protocol msg` |
| `Fix bug where path constraints are dropped` | `Path constraints dropped` |
| `Create new frontend handler for Go` | `Go frontend handler` |

- **Lead with the noun or area** ("Protocol …", "Solver …", "CLI …"), not filler verbs
- **Under 50 characters** — put detail in `--description`
- Avoid prefixes like "Add support for", "Update the", "Implement new"

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
- Merge to `main` directly — do NOT create pull requests
- After merge, delete the feature branch both locally and remotely
- Work is complete when changes are on `main` and pushed, not when a branch is pushed

### Commit messages and issue references

When a commit includes changes to beads data (`.beads/` files), list all
affected issue IDs in the commit message body. This makes it easy to trace
which issues were created, updated, or closed in each push.

Example:
```
Add symbolic integer support to concolic engine

Implements concrete+symbolic tracking for i32/i64 values with
path constraint generation for branch conditions.

Issues: str-a1b, str-c2d, str-e3f
```

### Parallel work with agent teams (preferred)

For parallel work on multiple issues, use Claude Code's **agent teams** instead
of manually running separate Claude instances in different worktrees. Teams
coordinate through the Task tool and each teammate runs in an isolated worktree
automatically (via `isolation: "worktree"` on the Task tool).

**Why teams over manual worktrees:**
- Claude manages concurrency — no Dolt database conflicts
- Teammates coordinate via task lists and messages
- Worktree lifecycle is fully automatic
- The team lead merges results back to `main`

**Supervision:** Always spawn teammates with `mode: "plan"`. Each teammate
must write a plan and get approval from the team lead before implementing.
This ensures the user can review proposed approaches before code is written.

**Example: working on two independent issues in parallel:**
```
1. Create tasks for each issue (TaskCreate)
2. Spawn teammates with isolation: "worktree", mode: "plan" (Task tool)
3. Each teammate researches and proposes a plan
4. Team lead reviews and approves/rejects each plan
5. Approved teammates implement, team lead merges results
```

### Worktree workflow (single-agent)

When working in a git worktree (e.g. started with `--worktree`), **always merge
the branch back into `main` before ending the session.** Do not leave the
worktree branch unmerged without asking.

**Before merging a worktree branch:**
- Preview conflicts first: `git merge --no-commit --no-ff <branch>` then `git merge --abort`
- Rebase long-lived worktrees onto main before merging: `git rebase main` (from the worktree)

**Worktree cleanup — do NOT do it manually:**
Claude Code manages worktree lifecycle automatically when invoked with
`--worktree`. **Never** run `git worktree remove` or delete the worktree
directory yourself — the user is prompted to keep or remove it when the session
ends. Manually removing the worktree mid-session can break the working directory
and cause confusing errors.
