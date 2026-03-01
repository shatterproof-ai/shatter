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

### Commit Early, Commit Often

Sessions can be interrupted at any time — context window compaction, usage limits,
crashes, or network failures. **Uncommitted work is lost work.**

- **Commit after every meaningful change.** Don't accumulate multiple logical
  changes before committing. If you've made something work — tests pass, a
  function is implemented, a bug is fixed — commit it immediately.
- **Push your branch frequently.** A local commit is better than uncommitted
  changes, but a pushed commit survives everything. Push after every 1–2 commits.
- **Never hold work "until it's ready."** A series of small, incremental commits
  on a feature branch is vastly better than one big commit that might never land.
- **Teammates in worktrees**: push your branch before reporting task completion.
  If the session is interrupted before the lead merges your branch, pushed work
  can be recovered. Unpushed work in a worktree cannot.

### Commit messages and issue references

When a commit includes changes to beads data (`.beads/` files), list all
affected issue IDs in the commit message body. This makes it easy to trace
which issues were created, updated, or closed in each push.

Example for feature commits:
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

**Quick start:** Use `/swarm` to automate the full parallel workflow — triage,
team setup, plan review, merge, quality gates, and close protocol.

**Manual workflow (if not using `/swarm`):**
```
1. Create tasks for each issue (TaskCreate)
2. Spawn teammates with isolation: "worktree", mode: "plan" (Task tool)
3. Each teammate researches and proposes a plan
4. Team lead reviews and approves/rejects each plan
5. Approved teammates implement, team lead merges results
```

### Worktree isolation for teammates

When spawned as a teammate with `isolation: "worktree"`, you run in a separate
copy of the repo. **Violating isolation corrupts other agents' work.**

**Rules:**
- **Never `cd` to the main repo** (`/home/ketan/project/shatter`). Stay in your
  worktree directory (under `.claude/worktrees/`).
- **Never `git checkout`** to switch branches. You are already on your branch.
- **Never run git commands with `-C /home/ketan/project/shatter`**. All git
  operations must target your worktree.
- **Verify isolation** before critical operations: `git rev-parse --show-toplevel`
  must return a path containing `.claude/worktrees/`.
- **Commit and push only your branch**. The team lead merges to main.

**Why this matters:** All worktrees share the same `.git` directory. Running
`git checkout` in the main repo changes HEAD for the main working directory,
which other agents and the team lead depend on. This causes branch switching
races, lost commits, and merge failures.

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

<!-- BEGIN BEADS INTEGRATION -->
## Issue Tracking with bd (beads)

**IMPORTANT**: This project uses **bd (beads)** for ALL issue tracking. Do NOT use markdown TODOs, task lists, or other tracking methods.

### Why bd?

- Dependency-aware: Track blockers and relationships between issues
- Git-friendly: Auto-syncs to JSONL for version control
- Agent-optimized: JSON output, ready work detection, discovered-from links
- Prevents duplicate tracking systems and confusion

### Quick Start

**Check for ready work:**

```bash
bd ready --json
```

**Create new issues:**

```bash
bd create "Issue title" --description="Detailed context" -t bug|feature|task -p 0-4 --json
bd create "Issue title" --description="What this issue is about" -p 1 --deps discovered-from:bd-123 --json
```

**Claim and update:**

```bash
bd update bd-42 --status in_progress --json
bd update bd-42 --priority 1 --json
```

**Complete work:**

```bash
bd close bd-42 --reason "Completed" --json
```

### Issue Types

- `bug` - Something broken
- `feature` - New functionality
- `task` - Work item (tests, docs, refactoring)
- `epic` - Large feature with subtasks
- `chore` - Maintenance (dependencies, tooling)

### Priorities

- `0` - Critical (security, data loss, broken builds)
- `1` - High (major features, important bugs)
- `2` - Medium (default, nice-to-have)
- `3` - Low (polish, optimization)
- `4` - Backlog (future ideas)

### Workflow for AI Agents

1. **Check ready work**: `bd ready` shows unblocked issues
2. **Claim your task**: `bd update <id> --status in_progress`
3. **Work on it**: Implement, test, document
4. **Discover new work?** Create linked issue:
   - `bd create "Found bug" --description="Details about what was found" -p 1 --deps discovered-from:<parent-id>`
5. **Complete**: `bd close <id> --reason "Done"`

### Auto-Sync

bd automatically syncs with git:

- Exports to `.beads/issues.jsonl` after changes (5s debounce)
- Imports from JSONL when newer (e.g., after `git pull`)
- No manual export/import needed!

### Important Rules

- ✅ Use bd for ALL task tracking
- ✅ Always use `--json` flag for programmatic use
- ✅ Link discovered work with `discovered-from` dependencies
- ✅ Check `bd ready` before asking "what should I work on?"
- ❌ Do NOT create markdown TODO lists
- ❌ Do NOT use external issue trackers
- ❌ Do NOT duplicate tracking systems

For more details, see README.md and docs/QUICKSTART.md.

<!-- END BEADS INTEGRATION -->
