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
3. For each task, spawn a teammate via the `Agent` tool with:
   - `subagent_type: "general-purpose"`
   - `mode: "plan"`
   - `team_name: <team>`
   - Do NOT use `isolation: "worktree"` — it is unreliable. Instead, include
     manual worktree setup commands in the prompt (see below).
4. Each teammate's prompt must include:
   - The full issue details (from `bd show`)
   - Instruction to follow CLAUDE.md and AGENTS.md conventions
   - **Manual worktree setup** (copy verbatim into each prompt):
     ```
     ## WORKTREE SETUP — RUN FIRST
     Before doing anything else, create and enter an isolated worktree:
     ```bash
     BRANCH_NAME="<issue-id>"
     git worktree add /home/ketan/project/shatter/.claude/worktrees/$BRANCH_NAME -b $BRANCH_NAME
     cd /home/ketan/project/shatter/.claude/worktrees/$BRANCH_NAME
     ```
     ALL subsequent commands must run from this worktree directory. Verify with:
     ```bash
     pwd  # Must show .claude/worktrees/<issue-id>
     git branch --show-current  # Must show <issue-id>
     ```
     NEVER cd back to /home/ketan/project/shatter. Commit and push ONLY on
     your worktree branch — never touch main.
     ```
   - **Pre-completion enforcement** (copy verbatim into each prompt):
     ```
     ## PRE-COMPLETION — MANDATORY
     You MUST run `/pre-completion` and include the full summary table in your
     completion message. Your work WILL BE REJECTED if the table is missing or
     shows FAIL. Do NOT send a completion message without it.

     After pre-completion passes, push your branch:
     ```bash
     git push -u origin <branch-name>
     ```
     Then send your completion message with the table to the team lead.
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

1. **Verify quality**: Confirm the teammate's completion message includes the
   `/pre-completion` summary table with `Pre-completion: PASS`. Reject any
   completion announcement that lacks the table or shows FAIL status — send
   the teammate back to fix the issues. Do NOT accept "done" without the table.
2. **Get the branch**: The branch name matches the issue ID (from manual worktree setup)
3. **Merge to main**:
   ```bash
   git checkout main  # Verify with git branch --show-current
   git merge <branch> --no-edit
   ```
4. **Handle conflicts**: If merge conflicts arise, resolve them or ask the teammate to rebase
5. **Close the beads issue**: `bd close <id>`
6. **Clean up worktree and branch**:
   ```bash
   git worktree remove .claude/worktrees/<issue-id>
   git branch -d <issue-id>
   ```
7. **Only then** send `shutdown_request` to the teammate

**CRITICAL**: Never shut down a teammate before merging their branch. Always
merge first, then clean up worktree, then shut down.

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
