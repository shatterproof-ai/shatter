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
- **Bug fixes require a reproduction test first** — write an automated test that demonstrates the bug (must fail), then fix the code and verify the test passes. Never attempt a fix without a failing test.
- **No magic numbers or string literals** — define named constants for default values, timeouts, error codes, capability lists, and any value that appears in both production code and tests. Tests must reference the constant, not duplicate the literal. Each language has a canonical location for constants (see per-language conventions below).

See `/rust-conventions`, `/ts-conventions`, `/go-conventions` skills for detailed per-language standards.

### Inline Documentation

Comments explain **why** or document **non-obvious contracts** — never restate what the code already says. If a name needs a comment, choose a better name first.

**What to document:**
- Public API contracts: preconditions, postconditions, error behavior, ownership semantics
- Non-obvious design choices: why an algorithm was chosen, why a field exists, why ordering matters
- Known-answer test fixtures: expected branches, triggering inputs, and edge cases (see `examples/go/04-nested-control-flow.go` for the model)
- `#[allow(...)]` / `@ts-ignore`: always explain why the suppression is needed

**What NOT to document:**
- What the code does when the code already says it (`// returns the sum` above `fn sum()`)
- Type information visible in the signature (`// takes a string` above `fn foo(s: string)`)
- Existence of language constructs (`// uses a switch statement`, `// is a simple struct`)

**The delete test:** If you can delete a comment and the code is equally clear, delete it.

**Bad → Good examples (from this codebase):**

```go
// BAD: restates syntax
// SwitchOnString uses a switch statement.
func SwitchOnString(color string) int {

// GOOD: documents the test contract
// SwitchOnString — 4 branches: "red"→1, "green"→2, "blue"→3, default→0.
// Analyzer should detect all four arms and the string-equality conditions.
func SwitchOnString(color string) int {
```

```go
// BAD: restates the type definition
// Point is a simple struct.
type Point struct {

// GOOD: no comment (the struct is self-describing). Or if it's testdata:
// Point — test fixture for struct-field access analysis.
type Point struct {
```

```rust
// BAD: restates the signature
/// Returns boundary values for the given type.
pub fn boundaries_for(ty: &ParamType) -> Vec<BoundaryValue> {

// GOOD: documents the contract
/// Returns boundary values applicable to `ty`, ordered by category
/// (limits first, then zeroes, then special values like NaN/empty).
pub fn boundaries_for(ty: &ParamType) -> Vec<BoundaryValue> {
```

### Test Tiers

| Tier | Command | Use when |
|---|---|---|
| Quick | `cargo test` | During development |
| Standard | `cargo test && cargo clippy -- -D warnings` | Before committing |
| Full | Standard + `cd shatter-ts && npm test` + `cd shatter-go && go test ./...` + `cd shatter-rust && cargo test` | Before merge or when touching protocol definitions |
| Walkthrough | `bash demo/walkthrough.sh --auto --delay 0` | After changing CLI output, protocol, frontend execution, or example files |

**Walkthrough gate**: The walkthrough exercises the full pipeline end-to-end (analyze, explore, scan, export, spec). Run it after any change to CLI commands, frontend handlers, protocol types, or example files. Errors in steps 1–19 (single-function explore, scan, cache, Go explore, export, run) indicate regressions — fix before merging. Errors in scan steps for `11-opaque-types.ts` and `12-external-deps.ts` are expected (opaque types and missing external modules).

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

### Frontend timeout contract

All frontends MUST read the `SHATTER_EXEC_TIMEOUT` env var (seconds) and apply it to function execution. The CLI sets this var from `--timeout` before spawning frontends.

| Frontend | Default | Env var | Implementation |
|---|---|---|---|
| Go | 5s | `SHATTER_EXEC_TIMEOUT` | `execTimeout()` in `instrument/executor.go` |
| TypeScript | 15s | `SHATTER_EXEC_TIMEOUT` | `getExecTimeoutMs()` in `src/executor.ts` |
| Rust | 5s | `SHATTER_EXEC_TIMEOUT` | `exec_timeout_from_env()` in `src/handler.rs` (stored, not yet applied — execute is unimplemented) |

Invalid values (non-numeric, zero, negative) fall back to the default silently.

## Output Review

After any change affecting CLI output, frontend logging, or protocol formatting, run `/walkthrough-review` to validate the output is human-readable.

Update README.md when build/run/config procedures change.

## Agent Workflow

See `AGENTS.md` for issue tracking (beads), git workflow, and agent operational instructions.

### Sprint Workflow

When asked to work on ready issues in parallel, **always invoke `/swarm`**. Do not manually re-implement the team/worktree workflow. The swarm skill handles triage, team setup, plan review, safe merge-before-shutdown, and quality gates.

### Research Memory

After researching codebase architecture or feature implementation status, save factual findings to project memory proactively — don't wait for the user to ask. Tag entries with date so stale facts can be identified later. This applies to any confirmed facts learned by reading code: what's implemented vs stubbed, how mechanisms work, which frontends support which features, etc.

### Plans

When a planning session produces a plan worth preserving, copy it from `~/.claude/plans/` into `docs/plans/` with a filename including the issue key and a descriptive name (e.g., `str-kapl-resilience-timeouts-memory.md`). Reference the plan from the relevant beads issue(s) via `--notes`.

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
