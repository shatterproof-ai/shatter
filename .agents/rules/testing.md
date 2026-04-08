# Testing Standards

## Non-Negotiable

Every agent — main or subagent — must run the full test and lint suite before declaring work done. There are zero exceptions. Skipping tests is never acceptable, regardless of time pressure, perceived simplicity, or "only changed one line" reasoning. If the tests cannot run in the current environment, that is a blocking problem to solve, not a reason to skip.

## Core Rules

- **Always write tests for new additions.** Tests ship with the code, not as a follow-up.
- **Update existing tests when modifying behaviour.**
- **Target >= 80% line coverage per package.** Glue/wiring packages are exempt.
- **Bug fixes require a failing regression test before any fix is implemented** — first write an automated regression test that reproduces the bug and fails in the current codebase. Only after that failing test exists may you implement the fix, then verify the regression test passes. Never attempt or commit a bug fix without a failing reproduction test.

## Test Quality

Coverage percentage is necessary but not sufficient. Tests must be:

- **Deterministic**: same input, same result, every time. No flaky tests. No dependence on wall-clock time, random values, or external service availability in unit tests. If a test fails intermittently, fix it immediately — flaky tests erode all confidence.
- **Fast**: unit tests complete in milliseconds, not seconds. If a test is slow, it is testing too much or has hidden I/O. Optimize or restructure.
- **Isolated**: each test runs independently. No shared mutable state between tests. No required execution order. Parallel-safe by default.
- **Meaningful**: test behavior, not implementation. A refactor that preserves behavior should not break tests. If it does, the tests are too tightly coupled.
- **Readable**: a failing test name and message must tell you what broke without reading the test body. Use descriptive names: `TestTransfer_InsufficientFunds_ReturnsError`, not `TestTransfer3`.

## Test Tiers

Define test tiers appropriate to the project. Typical structure:

| Tier | Use when | Typical time |
|---|---|---|
| Quick | During development | ~10-20s |
| Standard | Before committing | ~30-60s |
| Full | Before merge | ~2-5min |

Keep tests fast. If the full suite exceeds 5 minutes, investigate and fix. Slow tests get skipped — skipped tests hide bugs.

## Unit vs Integration

- **Unit tests**: no DB, no network. Fast and isolated.
- **Integration tests**: use real external dependencies (database, APIs). Guard with environment checks so they skip gracefully when dependencies aren't available.
- **Never mock the database in integration tests.** Compilation and unit tests alone are not sufficient — runtime issues like type encoding mismatches only surface with a live connection.

## Test Organization

- Unit tests go next to the code they test (e.g., `foo.go` -> `foo_test.go`, `Foo.test.tsx`)
- Extract testable logic from functions that have side effects into separate, pure functions
- Use table-driven tests where appropriate
- Reuse shared test generators and fixtures — don't reinvent per test file

## Mandatory Verification Gate

Before declaring any feature, bug fix, or task **done**, every agent must execute and confirm:

1. **All tests pass** — the full suite, not just the files you touched
2. **Linter clean** — zero warnings in the entire project, not just changed files
3. **Build succeeds** — compilation/build with no errors
4. **Cross-boundary tests pass** — if touching shared types, protocols, or APIs
5. **End-to-end pipeline works** — if the project has E2E tests and you touched the relevant code path

**Failure of any step is a hard stop.** Fix the issue before proceeding. Do not report partial success. Do not defer failures to another agent or follow-up task.

A feature is not done until it works end-to-end. Closing based on unit tests alone risks silent breakage in the wider system.
