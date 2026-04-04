# agents

Shared [Claude Code](https://claude.ai/code) rules and custom agent definitions
for use across projects.

- `rules/` are imported into project `CLAUDE.md` files via `@` directives
- `agents/` contains custom agent definitions discovered from `~/.claude/agents/`

These are two separate integration paths:

- **Project-local shared rules**: each project imports files from this repo
- **Global custom agents**: your machine exposes agent definitions from this
  repo through `~/.claude/agents/`

## Supported Project Setups

### Preferred: `.agents` git submodule

Add this repo to a project as a git submodule at `.agents/`, then import rules
from that path in the project's `CLAUDE.md`:

```bash
git submodule add git@github.com:ketang/agents.git .agents
git submodule update --init --recursive
```

```markdown
@.agents/rules/workflow.md
@.agents/rules/testing.md
@.agents/rules/go.md
```

This keeps the shared rules versioned with the consuming project while avoiding
ad hoc copies.

### Optional: sibling checkout

If you keep this repo checked out alongside a project, you can import rules from
the sibling path instead:

```markdown
@../agents/rules/workflow.md
@../agents/rules/testing.md
@../agents/rules/go.md
```

Example layout:

```
project/
  agents/          # this repo
  your-project/    # consuming project
```

## Available Rules

| File | What it covers |
|---|---|
| `rules/workflow.md` | Plan mode, subagents, verification, build-vs-buy, bug fixing, merge leases |
| `rules/testing.md` | Test standards, coverage targets, test-first bugs, completion checklist |
| `rules/code-quality.md` | Documentation, security basics, no magic numbers, no hardcoded paths |
| `rules/go.md` | Go conventions (error wrapping, slog, context, interfaces, SQL) |
| `rules/react-vite.md` | React / TypeScript / Vite / Mantine conventions |
| `rules/graphql.md` | gqlgen (backend) + gql.tada (frontend) patterns |
| `rules/database.md` | PostgreSQL / pgx / Goose migration patterns |
| `rules/beads.md` | Issue tracker policy, git workflow, plans |
| `rules/learning.md` | Knowledge tracking, error logging, self-maintenance of the knowledge system |

## Available Agents

| File | Model | Purpose |
|---|---|---|
| `agents/planner.md` | Opus | Task decomposition, model tagging, swarm-consumable output |
| `agents/task-coder.md` | Sonnet | Execute well-defined tasks with escalation and output control |

Agent definitions are `.md` files with YAML frontmatter that Claude Code discovers in `~/.claude/agents/`. Symlink or copy individual files there to make them available.

## Global Agent Setup

Symlink or copy individual files from `agents/` into `~/.claude/agents/` to
make those custom agents available globally.

## Landing Helper

This repo also ships a repo-agnostic merge helper at `bin/merge-with-lease`.
Run it from a consuming repository checkout to enforce an optimistic-
concurrency lease on `origin/main` while verifying the exact merge preview:

```bash
.agents/bin/merge-with-lease --branch <feature-branch> --verify "<required gate>" --push
```

If the repo is checked out as a sibling instead of a submodule, invoke the same
helper via `../agents/bin/merge-with-lease`.

## Setup

### Sibling checkout setup

```bash
cd ~/project  # or wherever your projects live
git clone git@github.com:ketang/agents.git
```

Then add `@import` lines to your project's `CLAUDE.md` for rules, and symlink
agent definitions into `~/.claude/agents/`.

### Submodule setup

From a consuming project:

```bash
git submodule add git@github.com:ketang/agents.git .agents
git submodule update --init --recursive
```

Then import rules from `@.agents/rules/...` in that project's `CLAUDE.md`.

## Troubleshooting

### `@.agents/...` imports do not resolve

The project-local `.agents` submodule is probably missing or uninitialized. Run:

```bash
git submodule update --init --recursive
```

### The project resolves imports but shared-rule changes look stale

The consuming project is pinned to an older submodule commit. To move its
`.agents` submodule to the latest `main` from this repo:

```bash
git submodule update --remote --merge .agents
```

Then commit the updated submodule pointer in the consuming project.

### Custom agents are missing globally

Check that `~/.claude/agents/` contains symlinks or copies of the desired files
from `agents/`. Project-local rule imports and global custom-agent discovery are
independent.
