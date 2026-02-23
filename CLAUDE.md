# Shatter v2

Automatic exploratory testing via concolic execution. Rust core engine with language-specific frontends (TypeScript, Go) communicating via JSON-over-stdio protocol.

See `PLAN.md` for the full architecture and implementation roadmap.
See `LANGUAGE-EVALUATION.md` for the rationale behind choosing Rust for the core.

## Project Structure

```
shatter-core/     Rust core engine (library crate)
shatter-cli/      Rust CLI binary (clap)
shatter-ts/       TypeScript frontend (Node.js subprocess)
shatter-go/       Go frontend (Go binary subprocess)
```

## Code Quality Standards

This project demands clean structure, high quality code, and thorough automated testing. These are not aspirational — they are requirements.

### Clean Structure
- Every module has a single, clear responsibility
- Public APIs are minimal and well-documented
- Dependencies flow in one direction: cli → core, frontends → protocol
- No circular dependencies between modules
- Prefer small, focused files over large monoliths

### High Quality Code
- All Rust code must pass `cargo clippy` with no warnings
- All TypeScript code must pass strict mode (`strict: true` in tsconfig)
- All Go code must pass `go vet` and `golangci-lint`
- No `unwrap()` in Rust library code — use proper error handling with `Result` and `?`
- No `any` type in TypeScript — use proper typing
- Name things precisely. If a name requires a comment to explain, choose a better name
- Keep functions short. If a function needs a section comment, extract a function instead

### Thorough Automated Testing
- Every module has unit tests. No exceptions
- Every public function has tests covering its documented behavior
- The concolic engine has integration tests with known-answer functions (functions where we know exactly which branches exist and what inputs trigger them)
- Frontend protocol handlers have round-trip tests (serialize → deserialize → verify)
- Test names describe the behavior being tested, not the function name
- Regression snapshots are checked into the repo and verified in CI

### Rust-Specific
- Run `cargo test` before every commit
- Run `cargo clippy -- -D warnings` before every commit
- Use `#[cfg(test)]` modules for unit tests in the same file
- Use `proptest` for property-based tests where applicable
- Prefer `thiserror` for error types in library code

### TypeScript-Specific
- Run `npm test` (jest) before every commit
- Use strict TypeScript — no implicit any, no unchecked index access
- Frontend protocol messages are validated against JSON schemas

### Go-Specific
- Run `go test ./...` before every commit
- Run `go vet` before every commit
- Use table-driven tests

## Issue Tracking

This project uses **beads** (`bd`) for issue tracking. Issue prefix is `str`.

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --status in_progress  # Claim work
bd close <id>         # Complete work
bd sync               # Sync with git
```

### Creating Issues

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
- After merge, delete the feature branch both locally and remotely
