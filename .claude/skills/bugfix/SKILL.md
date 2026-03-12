---
name: bugfix
description: Agent workflow for bug fixes. Enforces test-first discipline — write failing test, verify failure, implement fix, verify pass, run pre-completion checks.
user-invocable: true
---

# Bugfix Workflow: Test-First Discipline

Guides you through the mandatory bug fix workflow. Every bug fix must have a
reproduction test **before** the fix is attempted.

**Usage:** `/bugfix` or `/bugfix str-abc.1` (with issue key for commit messages)

---

## Phase 1 — Reproduce the Bug

1. **Understand the bug**: Read the issue description, error messages, and any
   reproduction steps. Identify the module and code path involved.

2. **Write a failing test** that demonstrates the bug:
   - The test must exercise the specific broken behavior
   - Place it in the appropriate test file for the module
   - Name it clearly (e.g., `test_issue_xyz_description`)

3. **Run the test and verify it fails**:
   ```bash
   # Run only the new test to confirm failure
   cargo test <test_name>          # Rust
   npm test -- --grep "<pattern>"  # TypeScript
   go test -run <TestName> ./...   # Go
   ```

4. **Verify it fails for the right reason**: Read the failure output. The test
   must fail because of the bug, not because of a typo or setup error. If the
   failure reason is wrong, fix the test before proceeding.

**Do not proceed to Phase 2 until you have a test that fails for the right reason.**

---

## Phase 2 — Implement the Fix

1. **Fix the root cause** — no temporary workarounds. Find the actual bug and
   correct it.

2. **Do not modify the test from Phase 1** during this phase. The test is your
   contract — if you need to change it, that means you misunderstood the bug.
   Go back to Phase 1.

3. **Keep changes minimal** — fix only what is broken. Do not refactor
   surrounding code, add features, or "improve" unrelated code.

---

## Phase 3 — Verify the Fix

1. **Run the reproduction test again** — it must now pass:
   ```bash
   cargo test <test_name>
   npm test -- --grep "<pattern>"
   go test -run <TestName> ./...
   ```

2. **Run the full test suite** for the affected module to check for regressions:
   ```bash
   cargo test                        # Rust core/cli
   cd shatter-ts && npm test         # TypeScript
   cd shatter-go && go test ./...    # Go
   cd shatter-rust && cargo test     # Rust frontend
   ```

3. **Check parallel code paths**: If the bug exists in one code path (e.g.,
   random explorer), check whether the same bug exists in the parallel path
   (e.g., concolic orchestrator). Fix both if needed.

---

## Phase 4 — Pre-Completion

Run `/pre-completion` to verify all quality gates, E2E tests, git status, and
scope. Do not declare the fix complete until `/pre-completion` reports PASS.

---

## Commit Convention

If an issue key was provided, use it in commit messages:

```
<issue-key>: fix <description of what was broken>
```

Example: `str-abc.1: fix off-by-one in branch index negation`
