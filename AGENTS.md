# Agent Instructions

This project uses **bd** (beads) for issue tracking. Run `bd onboard` to get started.

## Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --status in_progress  # Claim work
bd close <id>         # Complete work
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

### Issue Shape: Prefer End-to-End Value

When breaking down a plan, default to **vertical slices**, not layer slices.
Each workable issue should deliver **one complete, testable unit of value**,
even if that unit is very small.

**The key test:** after merging this issue alone, can a user, teammate, or
agent actually do something new end-to-end? If not, the issue is probably cut
at the wrong seam.

**Good issue shape:**
- A thin end-to-end slice through the system for one narrow scenario
- May touch multiple layers (storage, API, CLI/UI) if that is what makes the
  scenario actually usable
- Produces a behavior that can be demonstrated, tested, and described in one
  sentence of user-facing value

**Bad issue shape:**
- One issue for database changes, one for API wiring, one for frontend wiring
  when none of them is useful alone
- Pure plumbing issues for a feature that only become valuable after several
  sibling issues land
- "First backend, then frontend, then integration" breakdowns for user-facing
  work

**Default rule:** if the work is a `feature`, split by **user journey, use case,
or behavior**, not by architecture layer.

**Examples:**

| Layer-sliced breakdown | End-to-end breakdown |
|---|---|
| `Add DB schema for saved filters` + `Add saved filters API` + `Build saved filters UI` | `Create a saved filter`, `Rename a saved filter`, `Delete a saved filter` |
| `Branch data Rust types` + `Branch data TS handler` + `Branch data Go handler` | `Scan shows branch source locations`, `Explore reports branch conditions`, `Export includes branch metadata` |
| `Add parser support` + `Add core model` + `Add CLI flag` | `CLI accepts one new config option and applies it during scan` |

**Exception:** layer- or infrastructure-shaped issues are acceptable only when
there is genuinely no smaller end-to-end slice worth landing yet. Those should
usually be `task` or narrowly scoped enabling work, not the default shape for a
feature plan.

### Issue Sizing: Keep Issues Small

Every issue should represent **one focused, independently completable unit of work**.
A well-sized issue can be implemented, tested, and merged in a single session.

**Target size:** 1–3 files changed, under ~200 lines of diff. If an issue looks
bigger, break it down.

**How to tell an issue is too big:**
- The description has multiple distinct deliverables or bullet points
- You find yourself thinking "first I need to X, then Y, then Z" — each of those is its own issue
- The acceptance criteria have more than 3 items
- It bundles multiple user journeys or behaviors into one issue
- The issue only delivers partial plumbing and depends on sibling issues before
  any value appears

**What to do instead:**
- Create an **epic** for the larger goal, then create small child issues under it
- Each child issue should be one **end-to-end behavior**: "create one saved
  filter", "scan one new file pattern", "export one new metadata field"
- Use `--parent` and `--waits-for` to link children to the epic
- Use `bd dep add` for ordering constraints between children

**Examples:**

| Too big | Break into |
|---|---|
| `Add symbolic integer support` | Epic + `Int equality branches work end-to-end`, `Int comparisons solve end-to-end`, `Int arithmetic flows through explore` |
| `Protocol message for branch data` | `Explore reports branch source lines`, `Spec JSON includes branch metadata`, `Scan forwards branch metadata to reports` |
| `Fix and test path constraint bug` | `Repro test for path constraints`, `Fix path constraint drop` |

**Rule of thumb:** If you can't describe the issue in one sentence without "and",
it's two issues.

**Second rule of thumb:** If you can only describe the issue as a component
handoff like "add backend support" or "wire the frontend", it is probably not
an end-to-end issue yet.

**Always specify `--type` explicitly** — never rely on the default. Use the correct issue type:
- `feature` — delivers new user-visible or agent-visible capability (most issues are features)
- `bug` — something broken that needs fixing
- `epic` — a grouping container for related features/tasks. Not directly workable.
- `task` — narrow infrastructure chore with no user-visible effect (CI config, project init, doc-only changes). If it adds capability, it's a `feature`.

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

### Bug Fix Policy: Reproduce Before Fixing

All bugs must be reproduced in a good automated test **before** attempting any fix.
The workflow is:

1. Write a test that demonstrates the bug (the test must **fail**)
2. Verify the test fails for the right reason
3. Implement the fix
4. Verify the test now **passes**

This ensures every bug fix is validated by a regression test and prevents
"fixes" that don't actually address the root cause.

### Completing an Issue

An issue is not complete until:
1. All code changes pass quality gates (tests, clippy/lint, build)
2. Changes are committed on a **dedicated branch for this issue only** (no other issues' changes)
3. The branch is merged to `main` (one branch at a time — do not merge other branches simultaneously)
4. The branch is deleted after merge
5. The issue is closed with `bd close <id>`
6. Changes are pushed to remote with `git push`

Do not leave stale branches. Merge and delete promptly.

**Never** combine work for multiple issues on one branch. If you discover
additional work while on an issue branch, create a new issue and report it to
the team lead (or note it for later) — do NOT implement it yourself. Address
it on a separate branch after merging the current one. Scope creep across
agents is the primary cause of duplicate commits and orphan branches.

## Git Workflow

- Work on feature branches, not `main` directly
- **One branch per issue** — never combine multiple issues in a single branch
- Branch names should reference the issue: `str-<hash>-short-description`
- Commits should be clean and atomic — one logical change per commit
- Rebase feature branches onto `main` before merging
- **Merge one branch at a time** — merge branch A to `main`, push, then merge branch B. Never batch-merge multiple branches in a single operation.
- Merge to `main` directly — do NOT create pull requests
- After merge, delete the feature branch both locally and remotely
- Work is complete when changes are on `main` and pushed, not when a branch is pushed
- **Never cherry-pick commits.** Cherry-picking creates duplicate commits with
  different SHAs, making history confusing and leaving orphan branches that
  appear unmerged. Always merge or rebase entire branches instead.

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

When working on a beads issue, **prefix the commit message with the issue key**.
This links commits to the issue they address and makes history easy to trace.

Format: `<issue-key>: <description>`

Example for feature commits:
```
str-a1b: Add symbolic integer support to concolic engine

Implements concrete+symbolic tracking for i32/i64 values with
path constraint generation for branch conditions.
```

When a commit addresses multiple issues, use the primary issue as the prefix
and list others in the body:
```
str-a1b: Add symbolic integer support to concolic engine

Implements concrete+symbolic tracking for i32/i64 values with
path constraint generation for branch conditions.

Also: str-c2d, str-e3f
```

When a commit includes changes to beads data (`.beads/` files) without a
specific feature issue, list all affected issue IDs in the body:
```
bd sync: close str-a1b, str-c2d

Issues: str-a1b, str-c2d
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
3. Each teammate works on exactly ONE issue on its own branch
4. Each teammate researches and proposes a plan
5. Team lead reviews and approves/rejects each plan
6. Approved teammates implement, team lead merges results ONE AT A TIME
   (merge branch A → main → push, then merge branch B → main → push, etc.)
```

**Preventing duplicate work across agents:**
- The team lead must assign each issue to exactly one teammate. Never assign
  the same issue to multiple agents.
- Teammates must check `bd show <id>` before starting — if the issue is already
  `in_progress` or `closed`, stop and ask the lead for a different assignment.
- The team lead must mark issues `in_progress` (with assignee) before spawning
  the teammate, not after.

**Post-swarm cleanup:**
After all teammates finish and their branches are merged, the team lead must:
1. Delete all merged worktree branches: `git branch -d <branch>`
2. Verify no orphan branches remain: `git branch --no-merged main`
3. Clean up worktree directories if any remain under `.claude/worktrees/`

Orphan branches from interrupted swarms create confusion — they appear unmerged
but contain duplicate work that already landed on `main` via a different branch.

### Mandatory Pre-Completion Check

Agents MUST run **`/pre-completion`** before announcing work is done — whether
reporting to a team lead or to the user. This skill verifies quality gates, E2E
tests, commits pushed, scope, and acceptance criteria. Do not declare completion
until `/pre-completion` reports PASS.

The `/pre-completion` output table is proof of completion. Agents MUST include
it verbatim in their completion message. Team leads MUST reject completion
announcements that lack the table or show any FAIL status.

This is a hard requirement, not a suggestion. Agents that skip `/pre-completion`
risk announcing "done" with failing tests, unpushed commits, or scope creep —
all of which have caused regressions in this project.

### Worktree isolation for teammates

When spawned as a teammate with `isolation: "worktree"`, you run in a separate
copy of the repo. **Violating isolation corrupts other agents' work.**

**Rules:**
- **Never `cd` to the main repo**. Stay in your worktree directory (under
  `.claude/worktrees/`). If unsure, run `git rev-parse --show-toplevel`.
- **Never `git checkout`** to switch branches. You are already on your branch.
- **Never run git commands with `-C` pointing to the main repo**. All git
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
bd create "Issue title" --description="Detailed context" -t feature -p 0-4 --json  # ALWAYS specify -t
bd create "Issue title" --description="What this issue is about" -t bug -p 1 --deps discovered-from:bd-123 --json
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

bd automatically syncs issue data to git:

- Exports to `.beads/issues.jsonl` after changes (5s debounce)
- Imports from JSONL when newer (e.g., after `git pull`)
- No manual export/import needed — use `bd export` if you need a one-off export

### Important Rules

- ✅ Use bd for ALL task tracking
- ✅ Always use `--json` flag for programmatic use
- ✅ Link discovered work with `discovered-from` dependencies
- ✅ Check `bd ready` before asking "what should I work on?"
- ❌ Do NOT create markdown TODO lists
- ❌ Do NOT use external issue trackers
- ❌ Do NOT duplicate tracking systems

For more details, see README.md.

<!-- END BEADS INTEGRATION -->
