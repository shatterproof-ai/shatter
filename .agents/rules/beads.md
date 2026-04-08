# Issue Tracking & Git Workflow

This file keeps the historical `beads.md` path for compatibility, but the
policy is tracker-agnostic. Each consuming project must document which issue
tracker it uses and the canonical commands or URLs for that tracker.

## Issue Tracking

```bash
# beads examples
bd list                                # all open issues
bd ready                               # issues with no blockers (ready to work)
bd show <id>                           # detailed view with dependencies
bd create --title "..." --description "..."
bd update <id> --status in_progress
bd close <id>

# GitHub Issues examples
gh issue list
gh issue view <id>
gh issue create --title "..." --body "..."
gh issue edit <id> --add-label in-progress
gh issue list --search '-label:"in-progress"'
gh issue edit <id> --remove-label in-progress
gh issue close <id>
```

Projects may use Beads, GitHub Issues, or another documented tracker. Agents
must follow the project-local tracker choice instead of assuming `beads`.

### Claiming Work

Agents must mark an issue as claimed before starting implementation so other
agents do not pick the same work.

- Beads: set status to `in_progress`.
- GitHub Issues: add the `in-progress` label.
- Another tracker: use the project's documented equivalent.

When looking for work, treat Beads issues with status `in_progress` and GitHub
issues labeled `in-progress` as unavailable unless the project documents an
override.

When work starts, claim the issue immediately. In practice this usually means
claiming it as soon as implementation begins on the dedicated feature branch +
worktree for that issue.

Do not close an issue just because branch work is finished. Close the issue
only after the completed, validated changes are merged into `main`.

If work is abandoned, handed off, or finishes on a branch without being merged
into `main`, remove or update the claim using the tracker's documented
mechanism instead of closing the issue.

For GitHub Issues, prefer a dedicated `in-progress` label over assignees. Use a
GitHub Projects status field only when the project explicitly documents it as
the canonical workflow.

### Issue Decomposition

Prefer **thin end-to-end vertical slices** that deliver a complete user or agent outcome. Do not split one behaviour into separate database, API, and frontend issues unless the work is a true standalone prerequisite.

### Agent Work Requires Issues

Any agent doing more than a one-liner (excluding project docs updates) must
link the work to an issue in the project's documented tracker and work in a
dedicated feature branch + worktree. This keeps work trackable and reversible.
Agents must claim that issue before implementation begins.

### Batch Commands

When reviewing multiple issues, prefer batched or parallel reads over repetitive
single-issue lookups when the tracker tooling supports it.
When selecting GitHub work, prefer queries that exclude `in-progress` issues.

## Git Workflow

When work is complete and verified, merge directly into `main` from the command line using an explicit merge commit. Do not create pull requests unless there is an unresolvable merge problem.

After the verified merge to `main` completes, close the corresponding issue
using the project's documented tracker workflow. Do not close issues before the
merge.

**Before `git merge`**: Always run `git branch --show-current` to verify you are on main. If not, `git checkout main` first.

**Merge policy**: Always merge completed work with `git merge --no-ff <feature-branch>`. Never fast-forward branch merges into `main`.

**No cherry-picking**: Never use `git cherry-pick`. If you need a change from another branch, merge or rebase instead. Cherry-picking creates duplicate commits, breaks bisect, and obscures history.

## Tracker Commands in Worktrees

If the project uses a tracker with repository-local state, run tracker commands
from the main repo and not from an attached worktree unless the tool explicitly
supports worktrees safely.

```bash
# from inside a worktree, change to the main repo first when required
cd /path/to/main-repo
bd list
```

Never create issues, update statuses, or close issues from a worktree when the
tracker tooling may resolve paths incorrectly or write to the wrong location.

## Implementation Plans

After plan mode approval, copy the plan from `~/.claude/plans/` into
`docs/plans/` with a meaningful name (for example,
`<project-prefix>-<issue-id>-description.md`). Plans in `~/.claude/` are
machine-local; the repo copy syncs across devices.

## Research Memory

After researching codebase architecture or feature implementation status, save factual findings to project memory proactively — don't wait to be asked. Tag entries with date so stale facts can be identified later.
