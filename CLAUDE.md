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

## Demo Walkthrough

`demo/walkthrough.sh` exercises shatter's full pipeline against example functions in `examples/typescript/src/`. It calls the CLI the same way a user would, so it serves as a living integration test. Steps that use unimplemented CLI commands will fail with an error until those commands are built — this is intentional.

When adding a new CLI command or flag, update the walkthrough to exercise it. The walkthrough should always reflect the current capabilities of the CLI.

## Agent Workflow

See `AGENTS.md` for issue tracking (beads), git workflow, and agent operational instructions.
