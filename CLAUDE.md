# Shatter v2

Automatic exploratory testing via concolic execution. Rust core engine with language-specific frontends (TypeScript, Go, Rust) communicating via JSON-over-stdio protocol.

See `PLAN.md` for the full architecture and implementation roadmap.

@.agents/rules/workflow.md
@.agents/rules/testing.md
@.agents/rules/code-quality.md
@.agents/rules/beads.md

Shared `.agents` rules are the baseline. When project-local instructions in
this file or `AGENTS.md` conflict with shared defaults, follow the Shatter
project-local instructions.

## Prerequisites

For local dev: Rust toolchain, Node.js 22+, Go 1.24+, libclang. Run `./scripts/configure-bindgen.sh` if Z3 build fails. Devcontainer includes everything.

## Code Quality Standards

Generic rules (magic numbers, hardcoded paths, parallel parity, file formats, security basics, inline documentation, bug-fix-test-first) are in the shared agent rules. Below are Shatter-specific standards only.

- All Rust code must pass `cargo clippy` with no warnings
- All TypeScript code must pass strict mode (`strict: true` in tsconfig)
- All Go code must pass `go vet` and `golangci-lint`
- No `unwrap()` in Rust library code — use `Result` and `?`
- No `any` type in TypeScript — use proper typing
- Dependencies flow in one direction: cli → core, frontends → protocol
- Integration tests use known-answer functions with expected branches and triggering inputs
- Frontend protocol handlers have round-trip tests (serialize → deserialize → verify)
- Regression snapshots are checked into the repo and verified in CI
- **Parallel parity in this project** means: `buildSymExpr` / `buildSymExprWithFlow`, random explorer / concolic orchestrator, CLI wiring for `--concolic` vs default. When adding a new AST node type, CLI flag, or config field, grep for the parallel code path.
- **Rust contracts (`#[requires]`, `#[ensures]`)** — use only where ALL THREE hold: (1) trust boundary (Z3 FFI, subprocess JSON, cross-collection indices), (2) type gap (can't encode as a type), (3) silent corruption (violation propagates as wrong results, not panics). Additionally, there must be a plausible bug the contract catches that proptest wouldn't. See `shatter-core/CLAUDE.md` for the full policy and qualifying sites.
- **Known-answer test fixtures**: document expected branches, triggering inputs, and edge cases (see `examples/go/04-nested-control-flow.go` for the model)

See `/rust-conventions`, `/ts-conventions`, `/go-conventions` skills for detailed per-language standards.

### Test Tiers

| Tier | Command | Use when |
|---|---|---|
| Quick | `npx task test-quick` | During development |
| Standard | `npx task test-standard` | Before committing |
| Full | `npx task check` | Before merge |
| E2E | `npx task e2e` | After pipeline changes |
| Smoke | `npx task smoke` | Before closing any issue |
| Walkthrough | `npx task walkthrough` | After changes to the compact demo path, walkthrough output, or walkthrough example set |
| Gauntlet | `npx task gauntlet` | After broad CLI coverage changes, demo-ineligible command additions, or gauntlet/example coverage changes |
| Parity | `npx task parity` | After changing frontend capability declarations, protocol registry, or adding a command handler |

**E2E gate**: The E2E concolic tests (`shatter-core/tests/e2e_concolic.rs`) run the real TS frontend subprocess through analyze → instrument → explore → Z3 solve. They are the **only tests that validate the full pipeline end-to-end**. Unit tests alone are insufficient — a module can pass all its own tests while being silently disconnected from the pipeline (see "Completion checklist" below). Run E2E tests after any change to:
- Solver logic (`solver.rs`, `string-ops.yaml`, `build.rs`)
- Instrumentor (`instrumentor.ts`, especially `buildSymExpr*` functions)
- Explorer or orchestrator (`explorer.rs`, `orchestrator.rs`)
- Protocol types that affect execute responses (`protocol.rs`, `protocol.ts`)
- CLI wiring that passes config to explorers (`main.rs`)

**Walkthrough gate**: The walkthrough is a compact demo, not a full command inventory. Keep it to roughly 8-15 steps and optimize for a coherent product story. The walkthrough must maintain parity across TypeScript, Go, and Rust for the core journey: each language should appear in equivalent demo steps for analyze, explore, scan, and one artifact/reporting path unless a documented product limitation prevents it. Run the walkthrough after changes to walkthrough output, walkthrough examples, or any user-facing behavior that materially changes the compact demo narrative.

**Gauntlet gate**: The gauntlet is the broad CLI and coverage path. Add feature probes, flag permutations, config variants, and other non-demo command coverage there instead of growing the walkthrough. Run the gauntlet after changes to CLI commands, frontend handlers, protocol-visible command behavior, or example coverage that falls outside the compact demo. The gauntlet prints an **ERROR SUMMARY** at the end and exits with code 1 if any step produced errors. Known exceptions: `stale` exit code 1 is informational (means "some functions are stale"), and scan errors for `11-opaque-types.ts` and `12-external-deps.ts` are expected.

### Formal Methods & Verification

Four complementary tools, each with a distinct role. PBT is the workhorse; the others fill gaps PBT can't reach.

| Tool | Role | When to use |
|---|---|---|
| **Property-based testing** | Invariant discovery, regression prevention | Any non-trivial public function with invariants |
| **Native fuzzing** | Crash resistance at parsing boundaries | Code that deserializes untrusted input |
| **Contracts** (`contracts` crate) | Runtime assertions at trust boundaries | Only where Rust's type system can't express the invariant (see below) |
| **Kani model checking** (deferred — P4) | Exhaustive verification of critical algorithms | Highest-stakes properties only (solver correctness). Not yet in use. |

#### Property-Based Testing (primary strategy)

Every component uses PBT: **proptest** (Rust), **fast-check** (TypeScript), **rapid** (Go). PBT is not optional decoration — it is a primary testing strategy alongside unit tests and E2E tests.

**When adding or modifying a public function**, add property tests that cover its core invariants:
- **Roundtrip properties**: serialize → deserialize → equality (table stakes — always include for serializable types)
- **Semantic invariants**: "output types match input types", "length is preserved", "ordering is maintained" — these catch real bugs that fixed examples miss
- **Pipeline composition**: test functions composed together, not just in isolation. The solver bridge (constraints → solve → overlay) and the explore loop (execute → classify → worklist) are especially important.
- **Negative properties**: malformed/adversarial input never causes panics

**Shared generators**: reuse `test_arbitraries.rs` (Rust), `arbSymExpr`/`arbTypeInfo` (TS), `genTypeInfo` (Go). Don't reinvent type generators per test file.

**Coverage target**: every module that handles untrusted input, crosses an FFI boundary, or maintains state should have PBT coverage of its core invariants — not just serialization.

See sub-crate CLAUDE.md files for per-component PBT priorities (`shatter-core`, `shatter-ts`, `shatter-go`).

#### Native Fuzzing

- **Go**: `testing.F` in `*_fuzz_test.go` files — byte-level mutation for crash/panic discovery at parsing boundaries. Seed corpus from existing test fixtures.
- **Rust**: `cargo-fuzz` for deserialization boundaries.
- Add a fuzz target for any code that deserializes untrusted input (protocol messages, subprocess JSON).

#### Contracts

High bar. Use `#[requires]`/`#[ensures]` only where ALL THREE hold: (1) trust boundary, (2) type gap, (3) silent corruption. See `shatter-core/CLAUDE.md` for the full policy, qualifying sites, and what does NOT qualify.

#### Anti-Patterns

- Contracts that restate type signatures — use the type system instead
- Proptest for trivial getters/setters — specific examples are clearer
- PBT that only tests serialization roundtrips without semantic invariants — roundtrips are table stakes, not the goal
- Duplicating generators across test files — use shared generators in `test_arbitraries.rs` / `arbSymExpr` / `genTypeInfo`

### Completion Checklist

In addition to the shared testing completion checklist (unit tests, linter, cross-boundary, E2E), Shatter requires:

1. **Property tests adequate** — if adding/modifying a public function, include proptest/fast-check/rapid properties covering its core invariants (not just serialization roundtrips)
2. **Cross-language tests pass** — if touching protocol types (Full tier)
3. **E2E pipeline works** — if touching any component in the analyze → instrument → execute → solve chain, run `cargo test --test e2e_concolic` and verify the pipeline still discovers expected branches
4. **Walkthrough passes** — if touching walkthrough output, walkthrough examples, or the compact demo flow
5. **Gauntlet passes** — if touching broad CLI coverage, non-demo command behavior, or gauntlet example coverage
6. **Parity contract updated** — if making a protocol-visible frontend change (new command handler, response field, error code, capability, or any observable behavior change), update the parity contract in the affected frontend's `CLAUDE.md` and add or adjust parity tests. *Output parity* (JSON wire format, response structure, error codes, observable behavior) is required across all frontends. *Implementation details* (internal types, helper functions, data structure choices) may differ between frontends. Run `npx task parity` (registry consistency + capability contract) and `npx task conformance` (wire format parity) to verify no unexpected drift.

**Why E2E matters:** This project has multiple code paths that process the same data (random explorer vs. concolic orchestrator, `buildSymExpr` vs. `buildSymExprWithFlow`, CLI wiring for different explorer modes). Features that work on one path are routinely broken on others. Closing an issue based on unit tests alone has repeatedly led to silent pipeline breakages. If the E2E tests don't cover your change, add a new E2E test case before closing.

## What NOT to Do

Security basics (secrets, build output, linter bypasses, hardcoded paths) are in the shared agent rules. Shatter-specific prohibitions:

- **Never edit generated protocol bindings** manually — regenerate from the schema
- **Never treat the walkthrough as the catch-all CLI inventory** — add a command to the walkthrough only if it materially improves the compact demo story. Otherwise add coverage to the gauntlet, conformance tests, E2E, or targeted command tests.
- **Never close a pipeline feature based on unit tests alone** — run `cargo test --test e2e_concolic` to verify end-to-end behavior
- **Never add a capability to one explorer path without checking the other** — the random explorer (`explorer.rs`) and concolic orchestrator (`orchestrator.rs`) are wired differently in `main.rs`. A feature added to one is routinely missing from the other (see str-emw6). Grep for the parallel path before declaring done.
- **Never change protocol-visible frontend behavior without updating the parity contract** — if a frontend change affects JSON output, error codes, response fields, or any observable behavior, update the parity contract in that frontend's `CLAUDE.md` and verify conformance (`npx task conformance`) before closing the issue. Internal refactors that leave JSON output identical do not require parity contract updates.

## Common Task Recipes

### Add a new protocol message type

Follow the full checklist in [`protocol/GOVERNANCE.md`](protocol/GOVERNANCE.md). Summary:

1. Update `protocol/registry.yaml`
2. Add/update JSON schemas in `protocol/schemas/`
3. Add valid + invalid fixtures in `protocol/fixtures/`
4. Define the message in `shatter-core/src/protocol.rs` (Rust types + serde)
5. Add round-trip serialization tests in the same module
6. Implement the handler in each frontend:
   - TypeScript: `shatter-ts/src/protocol.ts`
   - Go: `shatter-go/protocol/`
   - Rust: `shatter-rust/src/protocol.rs`
7. Add round-trip tests in each frontend (serialize → deserialize → verify)
8. Update conformance cases if adding a new command
9. **Update parity contracts** — if the new message type has any observable behavior difference or known drift across frontends, add a note to the parity contract in the relevant frontend `CLAUDE.md` files and document the drift in `protocol/conformance/conformance_cases.yaml` under `known_drifts`
10. Run all four validation checks (see governance doc), including `npx task conformance` to verify output parity

### Add a new CLI command

1. Add the clap subcommand in `shatter-cli/src/`
2. Implement the handler, delegating to `shatter-core` for logic
3. Add integration tests exercising the command
4. Decide whether the command belongs in the compact walkthrough or the gauntlet:
   - update `demo/walkthrough.sh` only if the command materially improves the compact demo story
   - otherwise update `demo/gauntlet.sh` or targeted test coverage

### Walkthrough vs. gauntlet

Use the compact walkthrough for the core Shatter story, not exhaustive coverage.

- Keep the walkthrough to roughly 8-15 steps.
- Preserve language parity across TypeScript, Go, and Rust for the core journey.
- Document any language-specific exception inline in the script when parity cannot be maintained.
- Prefer one representative command per capability cluster in the walkthrough.

Use the gauntlet for breadth.

- Add flag permutations, config-driven variants, stress cases, and non-essential command probes there.
- Docker and local walkthrough runners should cover the same compact step set as closely as practical.
- If a step cannot run in Docker because of missing mounts, persisted artifacts, or runtime prerequisites, fix the environment or move the step to gauntlet.

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

When asked to work on ready issues in parallel, **invoke `/swarm`** (the global skill handles team/worktree/merge mechanics). For epic-based work or Shatter-specific quality gates, also invoke `/swarm-project` which adds wave scheduling via `bd swarm` and runs `/check-all` + `/walkthrough-review`.

@shatter-core/CLAUDE.md
@shatter-cli/CLAUDE.md
@shatter-ts/CLAUDE.md
@shatter-go/CLAUDE.md
@shatter-rust/CLAUDE.md
