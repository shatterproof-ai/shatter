# This repo is a shared instructions container

`agents` holds shared rules and custom agent definitions consumed by other
projects. It is **not** a software project — there is no application code, no
build system, and no tests.

## What lives here

- `rules/` — Markdown rule files imported into project `CLAUDE.md` files via
  `@` directives
- `agents/` — Agent definition files (`.md` with YAML frontmatter) exposed
  globally via `~/.claude/agents/`

## How consuming projects use this repo

### Submodule setup (preferred)

```bash
# From the consuming project root
git submodule add <this-repo-url> .agents
git submodule update --init --recursive
```

Then import rules in the consuming project's `CLAUDE.md`:

```
@.agents/rules/workflow.md
@.agents/rules/beads.md
# ...add whichever rules apply
```

To update to the latest rules:

```bash
git submodule update --remote .agents
git add .agents
git commit -m "update agents submodule"
```

### Sibling checkout (alternative)

Keep a checkout at `../agents` relative to the consuming project and import:

```
@../agents/rules/workflow.md
```

### Custom agents

To make custom agents available to Claude Code globally:

```bash
ln -sf "$(pwd)/.agents/agents/<agent-name>.md" ~/.claude/agents/
```

Or copy instead of symlink if you want project-local pinning.

## What does NOT live here

- Application code
- Dependencies or package manifests
- CI/CD pipelines
- Project issue tracking state (this repo has no issue tracker)

## Contributing

This repo merges directly to `main`, but development work still happens on a
feature branch in a dedicated worktree. Keep the repo root on `main`; do not
repurpose the root checkout for implementation work. Creating the branch and
worktree for that task is required workflow and does not need separate user
approval. Do not commit on `main`. Do not create pull requests unless
explicitly instructed. This repo has no issue tracker, so rule and agent
changes are tracked with branches and commits.

Maintain a strict 1:1 relationship between scope, feature branch, and worktree.
One issue or explicitly approved task gets one branch and one dedicated
worktree. Do not reuse an old feature branch for a different task, and do not
repurpose an old worktree for a different task. If follow-up work is discovered
outside the current approved scope, start a new branch in a new worktree.

Do not switch a task worktree to `main`, and do not create a separate
"integration worktree" on `main` just to land a merge. A task worktree stays on
its task branch. If final landing requires a checkout on `main`, use the
project's designated primary checkout or documented landing helper instead of
creating a new `main` worktree.

When landing work in a consuming repository, prefer the shared optimistic-
concurrency helper:

```bash
bin/merge-with-lease --branch <feature-branch> --verify "<required gate>" --push
```

If the helper is imported as a submodule or sibling checkout, use that path
instead:

```bash
.agents/bin/merge-with-lease --branch <feature-branch> --verify "<required gate>" --push
../agents/bin/merge-with-lease --branch <feature-branch> --verify "<required gate>" --push
```

That helper captures a lease on `origin/main`, verifies the exact merge preview,
and aborts if `origin/main` moves before the merge commit is finalized.

If you need to execute the sequence manually, use:

```bash
git commit                  # commit local work on your feature branch first
git fetch origin
git rebase origin/main      # replay branch commits on top of the latest main
git push --force-with-lease # update the branch after rebasing
git checkout main
git pull --ff-only origin main
BASE_SHA=$(git rev-parse origin/main)
git merge --no-commit --no-ff <feature-branch>
<run required gate against the merge preview>
git fetch origin
test "$(git rev-parse origin/main)" = "$BASE_SHA"
git commit --no-edit
git push origin main
```

Preserve the repository's configured Git transport. If the checkout or remote
uses SSH and `git fetch`, `git pull`, or `git push` fails, do not switch the
remote to HTTPS as a fallback. If the checkout or remote uses HTTPS, do not
switch it to SSH as a fallback. Diagnose the actual auth, host, key, agent,
token, or network problem instead. Only change remote transport if the user
explicitly instructs it.

Never rebase or pull with staged or unstaged changes. Commit first. Never
validate against one `origin/main` SHA and merge after `origin/main` has moved.
Never fast-forward a feature branch into `main`.

When committing, use the `commit-commands:commit` skill. Do **not** use `commit-commands:commit-push-pr` — this repo has no PRs.
