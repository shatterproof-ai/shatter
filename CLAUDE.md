# Shatter v2

Automatic exploratory testing via concolic execution. Rust core engine with language-specific frontends (TypeScript, Go, Rust) communicating via JSON-over-stdio protocol.

See `PLAN.md` for architecture and `AGENTS.md` for beads tracking, git workflow, and agent operational instructions.

## Code Quality Standards

- Dependencies flow in one direction: cli → core, frontends → protocol
- **Parallel parity** in this project means `buildSymExpr` / `buildSymExprWithFlow`, random explorer / concolic orchestrator, CLI wiring for `--concolic` vs default. When adding a new AST node type, CLI flag, or config field, grep for the parallel code path before declaring done.
- Integration tests use known-answer functions with expected branches and triggering inputs (model: `examples/go/04-nested-control-flow.go`)
- Frontend protocol handlers have round-trip tests (serialize → deserialize → verify)
- Regression snapshots are checked into the repo and verified in CI

Per-language standards: `/rust-conventions`, `/ts-conventions`, `/go-conventions` skills. Formal methods / PBT / contracts policy: `/formal-methods-policy` skill and `shatter-core/CLAUDE.md`. Cross-frontend parity rules: `/frontend-parity` skill and `protocol/parity-matrix.yaml`.

### Test Tiers

| Tier | Command | Use when |
|---|---|---|
| Quick | `npx task test-quick` | During development |
| Standard | `npx task test-standard` | Before committing |
| Full | `npx task check` | Before merge |
| E2E | `npx task e2e` | After pipeline changes |
| Smoke | `npx task smoke` | Before closing any issue |
| Walkthrough | `npx task walkthrough` | After changes to the compact demo path, walkthrough output, or walkthrough example set |
| Gauntlet | `npx task gauntlet` | After broad CLI coverage changes or non-demo command additions |
| Parity | `npx task parity` | After changing frontend capability declarations, protocol registry, or adding a command handler |

**E2E gate.** `shatter-core/tests/e2e_concolic.rs` runs the real TS frontend subprocess through analyze → instrument → explore → Z3 solve. It is the only test suite that validates the full pipeline end-to-end — a module can pass its own unit tests while being silently disconnected from the pipeline, and this project has multiple parallel code paths (random explorer vs. concolic orchestrator, `buildSymExpr` vs. `buildSymExprWithFlow`, CLI wiring for different explorer modes) where features added to one path are routinely missing from another. Run E2E after any change to solver logic, instrumentor (`buildSymExpr*`), explorer/orchestrator, execute-response protocol types, or CLI wiring. If existing E2E cases don't cover your change, add one before closing.

**Walkthrough gate.** The walkthrough is a compact 8–15 step demo with language parity across TS/Go/Rust for analyze/explore/scan/reporting. Optimize for a coherent product story, not command coverage. Run after changes to walkthrough output, walkthrough examples, or the compact demo flow.

**Gauntlet gate.** Broad CLI and coverage probe — flag permutations, config variants, stress cases, non-essential command probes. Prints an ERROR SUMMARY and exits 1 on any errors. Known exceptions: `stale` exit 1 is informational; scan errors on `11-opaque-types.ts` and `12-external-deps.ts` are expected.

### Completion Checklist

Before declaring work done:

1. Unit tests + linter pass
2. **Property tests adequate** — new/modified public functions have proptest/fast-check/rapid coverage of core invariants, not just serialization roundtrips
3. **Cross-language tests pass** if touching protocol types (Full tier)
4. **E2E pipeline works** if touching any analyze → instrument → execute → solve component (`cargo test --test e2e_concolic`)
5. **Walkthrough passes** if touching walkthrough output or examples
6. **Gauntlet passes** if touching broad CLI coverage or non-demo command behavior
7. **Parity contract updated** if making a protocol-visible frontend change — update the affected frontend's `CLAUDE.md` and `protocol/parity-matrix.yaml`, then run `npx task parity` + `npx task conformance`. Internal refactors that leave JSON output identical do not require parity contract updates.

See the `/pre-completion` skill for the verification runner.

## What NOT to Do

- **Never edit generated protocol bindings manually** — regenerate from the schema
- **Never treat the walkthrough as the catch-all CLI inventory** — add to the walkthrough only if it materially improves the compact demo story; otherwise use the gauntlet, conformance tests, E2E, or targeted command tests
- **Never close a pipeline feature based on unit tests alone** — run `cargo test --test e2e_concolic`
- **Never add a capability to one explorer path without checking the other** — `explorer.rs` (random) and `orchestrator.rs` (concolic) are wired differently in `main.rs`; features added to one are routinely missing from the other (see str-emw6). Grep for the parallel path before declaring done.
- **Never change protocol-visible frontend behavior without updating the parity contract** — if JSON output, error codes, response fields, or observable behavior changes, update that frontend's `CLAUDE.md` and run `npx task conformance`

## Agent Workflow

### Subagent priming (nested CLAUDE.md)

Per-crate CLAUDE.md files (`shatter-core/`, `shatter-cli/`, `shatter-ts/`, `shatter-go/`, `shatter-rust/`) are **not** `@`-imported. Claude Code injects them on demand when an agent reads a file in the target subdirectory. **When dispatching a subagent to work on a specific crate, instruct it to `Read` one representative file in the target subtree before reasoning** — this ensures the crate's rules load before the subagent plans. A subagent that reasons purely from its prompt can miss parity contracts, timeout contracts, invocation-model dispatch, and other nested rules. See `~/dotfiles/claude/docs/nested-claude-md-loading.md` for the mechanism.

### Sprint Workflow

When asked to work on ready issues in parallel, invoke `/swarm` (handles team/worktree/merge mechanics). For epic-based work or Shatter-specific quality gates, also invoke `/swarm-project` which adds wave scheduling via `bd swarm` and runs `/check-all` + `/walkthrough-review`.

## Output Review

After any change affecting CLI output, frontend logging, or protocol formatting, run `/walkthrough-review` to validate the output is human-readable. Update README.md when build/run/config procedures change.

## References

- `protocol/GOVERNANCE.md` — checklist for adding/modifying protocol message types
- `protocol/parity-matrix.yaml` — authoritative cross-frontend capability matrix
- `AGENTS.md` — agent operational instructions, beads tracking, git workflow
- `PLAN.md` — architecture and implementation roadmap
- `~/dotfiles/claude/docs/nested-claude-md-loading.md` — CLAUDE.md auto-load mechanism
