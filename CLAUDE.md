# Shatter v2

Automatic exploratory testing via concolic execution. Rust core engine with language-specific frontends (TypeScript, Go, Rust) communicating via JSON-over-stdio protocol.

See `PLAN.md` for the full architecture and implementation roadmap.

## Prerequisites

For local dev: Rust toolchain, Node.js 22+, Go 1.24+, libclang. Run `./scripts/configure-bindgen.sh` if Z3 build fails. Devcontainer includes everything.

## Code Quality Standards

- All Rust code must pass `cargo clippy` with no warnings
- All TypeScript code must pass strict mode (`strict: true` in tsconfig)
- All Go code must pass `go vet` and `golangci-lint`
- No `unwrap()` in Rust library code — use `Result` and `?`
- No `any` type in TypeScript — use proper typing
- Dependencies flow in one direction: cli → core, frontends → protocol
- Integration tests use known-answer functions with expected branches and triggering inputs
- Frontend protocol handlers have round-trip tests (serialize → deserialize → verify)
- Regression snapshots are checked into the repo and verified in CI

See `/rust-conventions`, `/ts-conventions`, `/go-conventions` skills for detailed per-language standards.

### Test Tiers

| Tier | Command | Use when |
|---|---|---|
| Quick | `cargo test` | During development |
| Standard | `cargo test && cargo clippy -- -D warnings` | Before committing |
| Full | Standard + `cd shatter-ts && npm test` + `cd shatter-go && go test ./...` + `cd shatter-rust && cargo test` | Before merge or when touching protocol definitions |

## What NOT to Do

- **Never edit generated protocol bindings** manually — regenerate from the schema
- **Never commit** `.env` or files containing secrets — only `.env.example`
- **Never add** `node_modules/`, `dist/`, or `target/` to git
- **Never bypass clippy warnings** with `#[allow(...)]` without a comment explaining why
- **Never add a CLI command** without updating `demo/walkthrough.sh`

## Common Task Recipes

### Add a new protocol message type

1. Define the message in `shatter-core/src/protocol.rs` (Rust types + serde)
2. Add round-trip serialization tests in the same module
3. Implement the handler in each frontend:
   - TypeScript: `shatter-ts/src/protocol.ts`
   - Go: `shatter-go/protocol/`
   - Rust: `shatter-rust/src/protocol.rs`
4. Add round-trip tests in each frontend (serialize → deserialize → verify)

### Add a new CLI command

1. Add the clap subcommand in `shatter-cli/src/`
2. Implement the handler, delegating to `shatter-core` for logic
3. Add integration tests exercising the command
4. Update `demo/walkthrough.sh` to exercise the new command

### Add an integration test with known-answer functions

1. Write the target function in `examples/typescript/src/` (or the relevant language)
2. Document the expected branches and triggering inputs in a comment
3. Write the test in `shatter-core` that invokes the engine and asserts all branches are found
4. Check in a regression snapshot of the output

## Output Review

After any change affecting CLI output, frontend logging, or protocol formatting, run `/walkthrough-review` to validate the output is human-readable.

Update README.md when build/run/config procedures change.

## Agent Workflow

See `AGENTS.md` for issue tracking (beads), git workflow, and agent operational instructions.

### Sprint Workflow

When asked to work on ready issues in parallel, **always invoke `/swarm`**. Do not manually re-implement the team/worktree workflow. The swarm skill handles triage, team setup, plan review, safe merge-before-shutdown, and quality gates.

### Efficiency Rules

- **Batch `bd show` calls**: `bd show X && echo --- && bd show Y && echo --- && bd show Z` — never sequential individual calls.
- **Before `git merge`**: Always run `git branch --show-current` to verify you are on main. If not, `git checkout main` first.
- **After context compaction**: Trust the summary. Do not re-run git status, git diff, git log, or test suites that the pre-compaction portion already completed.
- **AskUserQuestion**: Only for decisions where the wrong choice requires significant rework. For preference questions, pick a sensible default and proceed.

@shatter-core/CLAUDE.md
@shatter-cli/CLAUDE.md
@shatter-ts/CLAUDE.md
@shatter-go/CLAUDE.md
@shatter-rust/CLAUDE.md
