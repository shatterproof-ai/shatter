---
name: swarm
description: Work all ready beads issues in parallel using agent teams with worktree isolation. Triages issues, spawns teammates, reviews plans, merges results, and runs quality gates.
user-invocable: true
---

# Swarm: Parallel Issue Work

Work `bd ready` issues in parallel using agent teams. Each teammate works in an
isolated worktree with plan-mode supervision.

## Usage

- `/swarm` — work all `bd ready` issues
- `/swarm str-abc str-def` — work specific issues only

---

## Phase 1 — Triage

1. Run `bd ready` (or use the provided issue IDs)
2. Run `bd show <id>` for each issue to understand scope
3. Identify issues that touch **overlapping files** — these must be sequenced, not parallelized
4. Skip issues that are too large or ambiguous for a single teammate (flag for manual work)
5. Present the triage plan to the user:
   - Which issues will be parallelized
   - Which issues are sequenced and why
   - Which issues are skipped and why

**Wait for user approval before proceeding.**

---

## Phase 2 — Team Setup

1. Create a team via `TeamCreate` (name: `swarm-YYYY-MM-DD` or similar)
2. Create a task (via `TaskCreate`) for each parallelizable issue, including the beads ID in the task description
3. For each task, spawn a teammate via the `Task` tool with:
   - `subagent_type: "general-purpose"`
   - `isolation: "worktree"`
   - `mode: "plan"`
   - `team_name: <team>`
4. Each teammate's prompt must include:
   - The full issue details (from `bd show`)
   - Instruction to run **`/pre-completion`** before reporting done — do not
     announce completion until it reports PASS
   - Instruction to **push their branch** before reporting completion
   - Instruction to follow CLAUDE.md and AGENTS.md conventions
   - **Worktree isolation rules** (copy verbatim into each prompt):
     ```
     ## WORKTREE ISOLATION — CRITICAL
     You are running in an isolated git worktree. You MUST:
     1. NEVER run `cd /home/ketan/project/shatter` or cd to the main repo
     2. NEVER run `git checkout` to switch branches — you are already on your branch
     3. ALWAYS use your current working directory for all operations
     4. Verify your worktree: run `git rev-parse --show-toplevel` — it must
        contain `.claude/worktrees/`, NOT be the main repo
     5. If any command changes your directory to the main repo, STOP and
        cd back to your worktree immediately
     6. Commit and push ONLY on your worktree branch — never touch main
     ```

---

## Phase 3 — Plan Review

For each teammate that submits a plan:

1. Review the plan for:
   - Correct scope (matches the issue, no scope creep)
   - Quality gates included
   - Test coverage plan (tests are comprehensive and well-targeted)
   - No overlap with other teammates' work
2. Approve or reject via `plan_approval_response`
3. If rejecting, provide specific feedback on what to change

---

## Phase 4 — Monitor & Merge

As teammates complete work:

1. **Verify quality**: Confirm the teammate ran quality gates and they passed
2. **Get the branch**: Read the worktree branch name from the Task result
3. **Merge to main**:
   ```bash
   git checkout main
   git merge <branch> --no-edit
   ```
4. **Handle conflicts**: If merge conflicts arise, resolve them or ask the teammate to rebase
5. **Close the beads issue**: `bd close <id>`
6. **Delete the branch**: `git branch -d <branch>`
7. **Only then** send `shutdown_request` to the teammate

**CRITICAL**: Never shut down a teammate before merging their branch. Worktree
cleanup deletes the branch — unmerged work is lost.

---

## Phase 5 — Quality Gates

After **all** teammates are merged:

1. Run `/check-all` for a full cross-language quality gate
2. If any failures, fix them on main directly
3. Run `/walkthrough-review` to verify the demo walkthrough:
   - Covers any new functionality added by the issues
   - Output is human-readable per the walkthrough criteria
   - If the walkthrough needs updates, make them

---

## Phase 6 — Close Protocol

1. Close any remaining beads issues: `bd close <id1> <id2> ...`
2. Sync and push:
   ```bash
   bd sync
   git pull --rebase
   git push
   git status  # Must show "up to date with origin"
   ```
3. Shut down all teammates (if not already done)
4. Delete the team via `TeamDelete`
5. Report summary to user:
   - Issues completed
   - Issues skipped or deferred
   - Any quality gate findings
   - Final git status
