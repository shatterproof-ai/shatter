---
name: pre-completion
description: Verify all completion criteria before declaring work done. Runs quality gates, E2E tests, checks walkthrough requirements, verifies commits are pushed, and confirms no scope creep.
user-invocable: true
---

# Pre-Completion: Done Criteria Verification

Verify all completion criteria are met before announcing work is done to the
team lead (if in a team) or the user (if working solo).
**Do not declare completion until this skill passes.**

**If any check fails, you are NOT done.** Fix the failing issue, then re-run
`/pre-completion` from the top. Repeat until all checks pass. Do not announce
completion, send a completion message, or report to the lead with any FAIL
status — go back and fix it first.

---

## Phase 1 — Determine What Changed

1. **Identify changed files**:
   ```bash
   git diff --name-only main...HEAD
   ```
   If no commits yet vs main, use `git diff --name-only HEAD` for staged/unstaged changes.

2. **Classify changes** into categories:
   - `rust-core` — files in `shatter-core/`
   - `rust-cli` — files in `shatter-cli/`
   - `typescript` — files in `shatter-ts/`
   - `go` — files in `shatter-go/`
   - `rust-frontend` — files in `shatter-rust/`
   - `protocol` — any `protocol.rs` or `protocol.ts` or `protocol/` files
   - `pipeline` — solver, instrumentor, explorer, orchestrator, or CLI wiring
   - `cli-output` — CLI commands, formatting, example files, demo scripts
   - `other` — docs, config, examples

---

## Phase 2 — Quality Gates

Run the appropriate language-specific gates based on what changed, plus E2E tests unconditionally.

### Language-specific (if the language was touched):

1. **Rust** (if `rust-core` or `rust-cli` changed):
   ```bash
   cargo test
   cargo clippy -- -D warnings
   ```

2. **TypeScript** (if `typescript` changed):
   ```bash
   cd shatter-ts && npm test
   cd shatter-ts && npx tsc --noEmit
   ```

3. **Go** (if `go` changed):
   ```bash
   cd shatter-go && go test ./...
   cd shatter-go && go vet ./...
   ```

4. **Rust frontend** (if `rust-frontend` changed):
   ```bash
   cd shatter-rust && cargo test
   cd shatter-rust && cargo clippy -- -D warnings
   ```

### Always required:

5. **Smoke test** — always run regardless of what changed:
   ```bash
   task smoke
   ```
   Fast (~15s) pipeline check covering TS and Go frontends. Catches gross
   pipeline breakages before the slower E2E tests.

6. **E2E concolic tests** — always run regardless of what changed:
   ```bash
   task e2e
   ```
   These are the only tests that validate the full pipeline end-to-end. A module
   can pass all its own tests while being silently disconnected from the pipeline.
   Use the task wrapper so fresh worktrees bootstrap the TypeScript frontend first.

### Conditional:

7. **Protocol sync** (if `protocol` changed):
   - Verify protocol types are consistent across all languages.
   - Run cross-language tests (Full tier).

8. **Gauntlet** (if `cli-output` changed):
   ```bash
   bash demo/gauntlet.sh --auto --delay 0
   ```
   Errors in steps 1-19 indicate regressions — fix before proceeding.

---

## Phase 3 — Code Quality Checks

1. **No magic numbers/string literals**: Verify constants are defined, not inline literals.
2. **No `unwrap()` in library code** (Rust): Verify `Result` and `?` are used.
3. **No `any` type** (TypeScript): Verify proper typing throughout.
4. **Parallel code path parity**: If the change touches one of a parallel pair
   (e.g., `buildSymExpr` / `buildSymExprWithFlow`, random explorer / concolic
   orchestrator, CLI wiring for `--concolic` vs default), verify the other path
   handles the same cases.
5. **Bug fix has reproduction test**: If this is a bug fix, verify a test exists
   that would fail without the fix.

---

## Phase 4 — Git Status

1. **All changes committed**:
   ```bash
   git status --porcelain
   ```
   - Working tree must be clean (no uncommitted changes).
   - If there are uncommitted changes, commit them first.

2. **Branch pushed** (if in a worktree or on a feature branch):
   ```bash
   git log --oneline origin/<branch>..HEAD 2>/dev/null || echo "no upstream"
   ```
   - All commits must be pushed. If unpushed commits exist, push now:
     ```bash
     git push -u origin HEAD
     ```

3. **Commit messages reference the issue**:
   - Verify commit messages follow `<issue-key>: <description>` format.

---

## Phase 5 — Scope Verification

1. **No scope creep**: Compare the changed files and functionality against the
   original issue description.
   - If you implemented something not in the issue, create a new beads issue for it
     and either revert or note the scope expansion.
   - Extra work discovered during implementation should be filed as new issues,
     not silently included.

2. **Issue acceptance criteria met**: Re-read the issue description and verify
   every acceptance criterion is satisfied.

---

## Output

Report a summary table. **This table IS the proof of completion.** You MUST
include it verbatim in your completion message to the team lead (via
`SendMessage`) or in your final response to the user. The lead will reject
completion announcements that do not include this table.

```
| Check                        | Status      | Notes                   |
|------------------------------|-------------|-------------------------|
| Rust tests                   | PASS / FAIL / N/A | ...              |
| Rust clippy                  | PASS / FAIL / N/A | ...              |
| TypeScript tests             | PASS / FAIL / N/A | ...              |
| TypeScript types             | PASS / FAIL / N/A | ...              |
| Go tests                     | PASS / FAIL / N/A | ...              |
| Go vet                       | PASS / FAIL / N/A | ...              |
| Protocol sync                | PASS / FAIL / N/A | ...              |
| E2E concolic                 | PASS / FAIL | ...                     |
| Walkthrough                  | PASS / FAIL / N/A | ...              |
| Parallel path parity         | PASS / N/A  | ...                     |
| Bug repro test exists        | PASS / N/A  | ...                     |
| All changes committed        | PASS / FAIL | ...                     |
| Branch pushed                | PASS / FAIL | ...                     |
| Commit messages valid        | PASS / WARN | ...                     |
| Scope matches issue          | PASS / WARN | ...                     |
| Acceptance criteria met      | PASS / FAIL | ...                     |

Pre-completion: PASS / FAIL
```

If any required check is FAIL, set overall status to **FAIL** and list what must
be fixed. **Do not stop here** — go fix the failures, then re-run `/pre-completion`.
You must keep iterating until the table shows `Pre-completion: PASS`. Only then
may you send a completion message.
