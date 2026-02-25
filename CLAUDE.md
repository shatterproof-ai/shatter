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

### Test Tiers

Pick the right tier for the moment:

| Tier | Command | Time | Use when |
|---|---|---|---|
| Quick | `cargo test` | ~5-15s | During development — catches logic bugs and regressions |
| Standard | `cargo test && cargo clippy -- -D warnings` | ~15-30s | Before committing — full Rust validation |
| Full | Standard + `cd shatter-ts && npm test` + `cd shatter-go && go test ./...` | ~30-60s | Before merge — validates all frontends too |

When working on a single frontend, run its tests alongside Rust tests. When
touching protocol definitions, always run Full — changes ripple across all
frontends.

## What NOT to Do

- **Never use `unwrap()`** in library code (`shatter-core`) — use `Result` and `?`
- **Never use `any`** in TypeScript — use proper typing
- **Never edit generated protocol bindings** manually — regenerate from the schema
- **Never commit** `.env` or files containing secrets — only `.env.example`
- **Never add** `node_modules/`, `dist/`, or `target/` to git
- **Never bypass clippy warnings** with `#[allow(...)]` without a comment explaining why
- **Never add a CLI command** without updating `demo/walkthrough.sh`

## Common Task Recipes

### Add a new protocol message type

1. Define the message in `shatter-core/src/protocol/` (Rust types + serde)
2. Add round-trip serialization tests in the same module
3. Implement the handler in each frontend:
   - TypeScript: `shatter-ts/src/protocol/`
   - Go: `shatter-go/protocol/`
4. Add round-trip tests in each frontend (serialize → deserialize → verify)

### Add a new CLI command

1. Add the clap subcommand in `shatter-cli/src/`
2. Implement the handler, delegating to `shatter-core` for logic
3. Add integration tests exercising the command
4. Update `demo/walkthrough.sh` to exercise the new command

### Add a new frontend language

1. Create `shatter-<lang>/` with the language's standard project structure
2. Implement the JSON-over-stdio protocol handler
3. Add round-trip tests for all existing protocol messages
4. Add the frontend to the Full test tier
5. Update the Project Structure table in this file

### Add an integration test with known-answer functions

1. Write the target function in `examples/typescript/src/` (or the relevant language)
2. Document the expected branches and triggering inputs in a comment
3. Write the test in `shatter-core` that invokes the engine and asserts all branches are found
4. Check in a regression snapshot of the output

## Demo Walkthrough

`demo/walkthrough.sh` exercises shatter's full pipeline against example functions in `examples/typescript/src/`. It calls the CLI the same way a user would, so it serves as a living integration test. Steps that use unimplemented CLI commands will fail with an error until those commands are built — this is intentional.

When adding a new CLI command or flag, update the walkthrough to exercise it. The walkthrough should always reflect the current capabilities of the CLI.

## README.md

`README.md` is the human-facing project documentation. **Keep it up to date**
when making changes that affect how someone builds, runs, or configures the
project (new prerequisites, new commands, changed project structure, etc.).

## Agent Workflow

See `AGENTS.md` for issue tracking (beads), git workflow, and agent operational instructions.

@shatter-core/CLAUDE.md
@shatter-cli/CLAUDE.md
@shatter-ts/CLAUDE.md
@shatter-go/CLAUDE.md
