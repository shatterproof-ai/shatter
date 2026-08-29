---
name: pre-completion
description: Verify all completion criteria before declaring work done. Runs diff-selected quality gates, checks walkthrough requirements, verifies commits are pushed, and confirms no scope creep.
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

## Phase 1 — Select Gates

Preview the exact gate set for the committed diff:

```bash
python3 scripts/affected-gates.py --base origin/main
```

Record the output verbatim. The selector unions every changed path, adds
pipeline/E2E and demo gates where required, and falls back to full `check` if
any path is unknown. A Git error is a failure, not an empty selection.

---

## Phase 2 — Quality Gates

Execute the selected gates serially through the governed task facade:

```bash
task affected
```

Copy its `Gates selected:` list into the output table. Do not substitute bare
language commands or add unconditional E2E: the selector emits the relevant
frontend E2E gates for pipeline paths, while `task check` remains the full
landing and CI backstop. In a swarm, the lead runs that full check once at
batch landing. A solo agent merging directly to `main` also runs the full
landing check before pushing.

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
| Gates selected               | PASS / FAIL | verbatim selector output |
| Rust tests                   | PASS / FAIL / N/A | ...              |
| Rust clippy                  | PASS / FAIL / N/A | ...              |
| TypeScript tests             | PASS / FAIL / N/A | ...              |
| TypeScript types             | PASS / FAIL / N/A | ...              |
| Go tests                     | PASS / FAIL / N/A | ...              |
| Go vet                       | PASS / FAIL / N/A | ...              |
| Protocol sync                | PASS / FAIL / N/A | ...              |
| E2E concolic                 | PASS / FAIL / N/A | selected frontend gates or N/A |
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
