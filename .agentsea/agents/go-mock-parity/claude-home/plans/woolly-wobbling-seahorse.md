# Plan: verify-agent command (kapow-n92.7)

## Context

The repo already has `scripts/classify-changes.sh` (maps changed files → risk surfaces) and `scripts/resolve-checks.sh` (maps surfaces → required commands with scope). The verify-agent command needs to orchestrate these into a single entrypoint that humans, hooks, and agents can invoke.

## Approach

Create `scripts/verify-agent.sh` — a shell script that:

1. Diffs against `main` (or `--base REF`) to get changed files
2. Pipes through `classify-changes.sh` to get surfaces
3. Pipes through `resolve-checks.sh` to get required commands + scope
4. Prints a summary (surfaces detected, scope, commands to run)
5. Executes each command sequentially, failing fast on first error
6. Reports pass/fail with actionable output
7. Exits 0 on success, non-zero on failure

### CLI interface

```
scripts/verify-agent.sh [OPTIONS]

Options:
  --base REF       Git ref to diff against (default: main)
  --dry-run        Show what would run without executing
  --verbose        Show command output even on success
  -h, --help       Show help
```

### Key behaviors

- **No changes detected**: print "No changes detected" and exit 0
- **Fail fast**: stop on first command failure, print which command failed and its output
- **Actionable output**: show surfaces, scope, and exact commands before running
- **GraphQL codegen check**: if `graphql-schema` surface is detected, verify generated files are up-to-date (run codegen, check for git diff)
- **Dry run**: print the plan without executing (useful for agents to preview)

## Files to create/modify

| File | Action |
|---|---|
| `scripts/verify-agent.sh` | **Create** — main orchestrator script |
| `Makefile` | **Edit** — add `verify-agent` target |
| `AGENTS.md` | **Edit** — document the verify-agent command |

## Implementation details

### scripts/verify-agent.sh

```
1. Parse args (--base, --dry-run, --verbose, --help)
2. REPO_ROOT=$(git rev-parse --show-toplevel)
3. Run classify-changes.sh --base $BASE_REF
4. If no surfaces → print "No changes" → exit 0
5. Run resolve-checks.sh with classification output
6. Parse scope and commands from JSON
7. Print summary: surfaces, scope, N commands
8. If --dry-run → print commands → exit 0
9. For each command:
   a. Print "Running: <command>"
   b. Execute from repo root, capture output
   c. On failure: print output, print "FAILED: <command>", exit 1
   d. On success: print "PASS: <command>"
10. Print "All checks passed" → exit 0
```

### Makefile target

```makefile
verify-agent:
	bash scripts/verify-agent.sh
```

## Verification

1. Run `scripts/verify-agent.sh --dry-run` from the worktree — should detect the new script files as ci-infra surface
2. Run `scripts/verify-agent.sh` — should execute the resolved checks and pass
3. Run `make test-quick` — quality gate for this issue
