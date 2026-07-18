# Agent Instructions

This project uses **bd** (beads) for issue tracking. The verified command basics live in the [bd Quick Reference](#bd-quick-reference) section below; the `bento:beads-issue-flow` skill covers the read → claim → update → close lifecycle. Below are Shatter-specific extensions.

## bd Quick Reference

These are the verified verbs for the lifecycle of an issue. There is **no `bd claim` subcommand** — claiming is a flag on `bd update`. Do not run `bd --help` to rediscover these; use the sequence below.

```bash
bd ready                       # list issues ready to work
bd show <id>                   # inspect an issue before starting
bd update <id> --claim         # atomically claim: sets assignee=you, status=in_progress (idempotent)
bd update <id> --status <s>    # change status (e.g. in_progress, blocked)
bd note <id> "..."             # append a progress note
bd close <id>                  # close a finished issue
```

Notes:
- `bd update <id> --claim` is the **only** correct way to claim. `bd claim`, `bd assign --self`, and `bd start` do not exist.
- Pass `--json` to any read command for programmatic use.
- bd writes can be slow; run them in the background when batching.

## Searching and Reading Files

Use the **Grep**, **Glob**, and **Read** tools — not shell `grep`/`cat`/`sed`/`find`/`ls` pipelines — for searching and reading files. The dedicated tools are faster, return structured results, respect `.gitignore`, and keep the context window clean. Reserve shell text utilities for cases where a dedicated tool genuinely cannot do the job (e.g. piping command output through a filter).

## Kapow Refute Workflow

For Kapow validation work that needs symbol-aware refactors, use the Shatter
wrapper instead of rediscovering Refute paths:

```bash
python3 scripts/kapow_refute_agent.py check
python3 scripts/kapow_refute_agent.py smoke
```

The wrapper targets `~/project/kapow/.agents/bin/refute` by default and uses
`~/project/refute` as the installer source. See
[`docs/validation/kapow-refute-agent-workflow.md`](docs/validation/kapow-refute-agent-workflow.md)
for install, smoke, and failure-mode details.

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

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until your issue branch has been merged into `main` and `git push origin main` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git commit                  # if the issue branch still has uncommitted work
   git fetch origin
   git rebase origin/main      # from the issue branch
   git push --force-with-lease # update the rebased issue branch
   git checkout main
   git pull --ff-only origin main
   git merge --no-ff <issue-branch>
   git push origin main
   git status  # MUST show clean working tree on main
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until the issue branch is merged into `main` and `git push origin main` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds

## Creating Issues

### Frontend and Protocol Issues

When creating an issue that touches a frontend, protocol message, or parallel code path, fill in the fields from [`protocol/frontend-issue-template.md`](protocol/frontend-issue-template.md) as the issue description. The template captures protocol-visible behavior changes, parity impact (which parallel paths need updating), default on/off state, and required test and contract updates — so reviewers can verify completeness without re-reading the diff.

### Issue Shape: Shatter-Specific Examples

The shared rules cover end-to-end vertical slicing. Here are Shatter-specific examples:

| Layer-sliced (bad) | End-to-end (good) |
|---|---|
| `Branch data Rust types` + `Branch data TS handler` + `Branch data Go handler` | `Scan shows branch source locations`, `Explore reports branch conditions`, `Export includes branch metadata` |
| `Add parser support` + `Add core model` + `Add CLI flag` | `CLI accepts one new config option and applies it during scan` |

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

### Completing an Issue

Run **`/pre-completion`** before declaring any issue complete. The skill
verifies quality gates, E2E tests, git status, scope, and acceptance criteria.
See the [Mandatory Pre-Completion Check](#mandatory-pre-completion-check)
section for the full policy.

Beyond passing `/pre-completion`, an issue is not complete until:
1. Changes are committed on a **dedicated branch for this issue only** (no other issues' changes)
2. The branch is merged to `main` (one branch at a time — do not merge other branches simultaneously)
3. The branch is deleted after merge
4. The issue is closed with `bd close <id>`
5. Changes are pushed to remote with `git push`

**For issues touching protocol-visible frontend behavior**: before declaring complete, verify the parity contract is up to date in the relevant `CLAUDE.md` files and parity tests pass (`task conformance`). *Output parity* (JSON wire format, response structure, error codes) must match across frontends. *Implementation details* (internal types, helper functions) may differ and do not require parity updates.

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
- **Never delete an unmerged branch without explicit user approval.** If a branch
  is not fully merged, stop and ask before deleting it locally or remotely.
- Work is complete when changes are on `main` and pushed, not when a branch is pushed
- **Never cherry-pick commits.** Cherry-picking creates duplicate commits with
  different SHAs, making history confusing and leaving orphan branches that
  appear unmerged. Always merge or rebase entire branches instead.

### Rebase Hygiene

Rebasing a worktree with uncommitted runtime artifacts produces `error: Your local changes to the following files would be overwritten by merge` and leaves the rebase half-applied. To avoid it:

- **Always run `git status --short` before `git rebase origin/main`** (or any merge/rebase). If the tree is dirty, deal with it deliberately — commit the changes that belong to your issue, or stash the rest — before rebasing.
- **Never blindly retry `git stash pop` into a dirty tree.** A failed `stash pop` means there is a conflict or an untracked-file collision; retrying does not resolve it and can clobber work. Inspect the conflict, resolve it, then pop once.
- The usual culprits are **build/run artifacts** that should not be committed: `target/`, `node_modules/`, `dist/`, `*.tsbuildinfo`, `.shatter/` run output, and demo/gauntlet scratch files. Add them to `.gitignore` or remove them; do not commit them just to make a rebase succeed.
- **Do not blanket-ignore `.beads/`.** `bd sync` intentionally commits `.beads/*.jsonl` — those files are tracked on purpose. Ignore only the specific artifacts above, never the whole `.beads/` directory.

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
coordinate through the Task tool and each teammate works on its own branch in a
linked worktree under `~/.local/share/worktrees/shatter/`.

**Why teams over manual worktrees:**
- Claude manages concurrency — no Dolt database conflicts
- Teammates coordinate via task lists and messages
- The team lead merges results back to `main`

**Worktree setup is not automatic for background teammates.** `isolation:
"worktree"` on a *background* Agent/Task call creates the worktree but does NOT
set the agent's working directory to it — background agents wake up in the
primary checkout on `main`. Always include explicit worktree setup in the
teammate prompt (`git worktree add -b <branch> ~/.local/share/worktrees/shatter/<branch> main`,
then `cd` into it) and have the teammate verify with
`swarm-worktree-verify.py --require-linked-worktree` before making any edit.
Foreground teammates spawned via `TeamCreate` land in their worktree directly.

**Supervision:** For well-scoped issues with clear acceptance criteria, spawn
teammates with `mode: "acceptEdits"` (or `"bypassPermissions"`) so they proceed
without a serial plan-approval bottleneck. Reserve `mode: "plan"` for genuinely
ambiguous or risky issues whose approach should be reviewed before code is
written. Spawn teammates in the **foreground** so they appear as visible tmux
panes; background agents silently stall on permission prompts.

**Team-lead liveness:** The team lead must actively poll teammate liveness
(e.g. `Monitor` / heartbeat checks) and auto-resume or report a stalled or
crashed teammate — never idle until the user notices. A swarm where the lead
waits passively for teammates that have died is a stalled swarm; detect it and
act.

**Quick start:** Use the `bento:swarm` skill to automate the full parallel
workflow — triage, team setup, worktree isolation, plan review, merge, quality
gates, and close protocol.

**Manual workflow (if not using `bento:swarm`):**
```
1. Create tasks for each issue (TaskCreate)
2. Spawn teammates in the foreground with mode: "acceptEdits", including
   explicit worktree setup in the prompt (see above); one issue per teammate
3. Each teammate works on exactly ONE issue on its own branch
4. Team lead polls teammate liveness and resumes any that stall
5. Teammates implement; team lead merges results ONE AT A TIME
   (merge branch A → main → push, then merge branch B → main → push, etc.)
```
For genuinely ambiguous issues, spawn with `mode: "plan"` instead and review
each teammate's plan before it implements.

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
3. Clean up worktree directories if any remain under `~/.local/share/worktrees/shatter/`

If any branch appears in `git branch --no-merged main`, do not delete it unless
the user explicitly approves that deletion.

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
  `~/.local/share/worktrees/shatter/`). If unsure, run `git rev-parse --show-toplevel`.
- **Never `git checkout`** to switch branches. You are already on your branch.
- **Never run git commands with `-C` pointing to the main repo**. All git
  operations must target your worktree.
- **Verify isolation** before critical operations: `git rev-parse --show-toplevel`
  must return a path containing `~/.local/share/worktrees/shatter/`.
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

<!-- Beads command basics are in the "bd Quick Reference" section above and the bento:beads-issue-flow skill.
     Shatter-specific beads extensions: issue types always specify -t, use discovered-from for linked bugs,
     always use --json for programmatic use. -->


<!-- headroom:rtk-instructions -->
# RTK (Rust Token Killer) - Token-Optimized Commands

When running shell commands, **always prefix with `rtk`**. This reduces context
usage by 60-90% with zero behavior change. If rtk has no filter for a command,
it passes through unchanged — so it is always safe to use.

## Shared-Machine Resource Etiquette (str-35vtk.5)

Many agents build and test concurrently on one machine. Governance is wired
into the repo — do not work around it:

- **Heavyweight gates take a machine-wide slot.** `check`, `check-fast`,
  `e2e*`, `conformance`, `parity`/golden, `walkthrough`, `gauntlet`, and
  `core:test-ignored` run through `scripts/gate-wrapper.sh`: a counting
  semaphore of `SHATTER_HEAVY_SLOTS` flock slots (default `nproc/8`, min 1)
  under `${XDG_RUNTIME_DIR:-/tmp}/shatter-heavy-slots/`, plus `nice`/`ionice`.
  A queued gate prints "waiting for a heavyweight slot" — that is normal;
  do not kill it and do not bypass the wrapper by running the underlying
  `cargo test --test e2e_*` / harness commands bare when an equivalent task
  exists.
- **Parallelism budgets.** `.cargo/config.toml` caps cargo at `jobs = 8` and
  test threads at 4 (`force = false` — a pre-set `RUST_TEST_THREADS` wins);
  the root Taskfile defaults `GOFLAGS=-p=4` and `GOMAXPROCS=8`. Solo humans
  and CI reclaim the machine by exporting `CARGO_BUILD_JOBS`,
  `RUST_TEST_THREADS`, `GOFLAGS`, `GOMAXPROCS`, or `SHATTER_HEAVY_SLOTS`.
- **Gate timings are recorded** to `~/.cache/shatter/gate-times.csv`
  (timestamp, worktree, gate, wall seconds, exit code, loadavg, slot) —
  budgets live in `docs/perf/gate-budgets.md`.
- Slow builds under load are contention, not code failure: re-run before
  debugging (see docs/perf/gate-budgets.md for the baseline flake analysis).

## Key Commands
```bash
# Git (59-80% savings)
rtk git status          rtk git diff            rtk git log

# Files & Search (60-75% savings)
rtk ls <path>           rtk read <file>         rtk grep <pattern>
rtk find <pattern>      rtk diff <file>

# Test (90-99% savings) — shows failures only
rtk pytest tests/       rtk cargo test          rtk test <cmd>

# Build & Lint (80-90% savings) — shows errors only
rtk tsc                 rtk lint                rtk cargo build
rtk prettier --check    rtk mypy                rtk ruff check

# Analysis (70-90% savings)
rtk err <cmd>           rtk log <file>          rtk json <file>
rtk summary <cmd>       rtk deps                rtk env

# GitHub (26-87% savings)
rtk gh pr view <n>      rtk gh run list         rtk gh issue list

# Infrastructure (85% savings)
rtk docker ps           rtk kubectl get         rtk docker logs <c>

# Package managers (70-90% savings)
rtk pip list            rtk pnpm install        rtk npm run <script>
```

## Rules
- In command chains, prefix each segment: `rtk git add . && rtk git commit -m "msg"`
- For debugging, use raw command without rtk prefix
- `rtk proxy <cmd>` runs command without filtering but tracks usage
<!-- /headroom:rtk-instructions -->
