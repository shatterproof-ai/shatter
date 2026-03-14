# Plan: flt-it8.32.12 — Issue Closure Gate

## Context

Flotsam's governance framework has individual CI scripts (`detect-risk.py`, `check-doc-commands.sh`, `check-placeholders.py`) and skill-based workflows (`/ship-gates`, `/security-review`), but no single automated script that validates all closure criteria before `bd close`. This task creates that orchestrator and documents beads label conventions.

## Deliverables

### 1. Create `scripts/ci/close-issue-check.sh` (NEW)

Bash orchestrator that runs pre-closure checks. Follows `check-doc-commands.sh` patterns exactly.

**Structure:**
- `set -euo pipefail`, color helpers with terminal detection, `pass()`/`fail()`/`warn()`/`header()` functions
- `REPO_ROOT` resolved via `$(cd "$(dirname "$0")/../.." && pwd)`
- Error/warning counters

**Arguments:**
- `--tests` — check test companions for new source files
- `--docs` — run `check-doc-commands.sh`
- `--placeholders` — run `check-placeholders.py`
- `--risk` — run `detect-risk.py`
- No flags = run all checks
- `--strict` or `FLOTSAM_STRICT=1` — exit 1 on failures (advisory otherwise)
- `--base <ref>` — base ref for diffs (default: auto-detect `origin/main` or `main`)
- `--markdown` — suppress colors, output markdown-formatted report
- `--help`

**Check functions:**
1. `check_risk()` — run `detect-risk.py`, report risk classes, suggest labels
2. `check_tests()` — diff changed files, check for companion test files (Go: `*_test.go` in same dir; Web: `*.test.{ts,tsx}`). Exempt: generated, cmd/, router/, type defs
3. `check_docs()` — run `check-doc-commands.sh`, capture result
4. `check_placeholders()` — run `check-placeholders.py`, capture result
5. `check_todo_markers()` — grep changed non-test files for TODO/FIXME/HACK/XXX/StatusNotImplemented

**Exit logic:** Advisory by default (exit 0 with warnings). Strict mode exits 1 if ERRORS > 0.

### 2. Update `docs/policies/issue-closure.md`

- Add "Automated Closure Check" section referencing the script with usage examples
- Add "Beads Labels" section documenting three labels:
  - `security-review-required` — trust boundary changes
  - `docs-impact` — user-facing documentation changes
  - `walkthrough-required` — complex multi-package changes
- Update closure checklist to include script and labels

### 3. Update `.claude/skills/ship-gates/SKILL.md`

- Step 5: Add `close-issue-check.sh --markdown` invocation before manual questions
- Quick Reference: Replace manual steps 3-4 with single `close-issue-check.sh` call

### 4. Update `AGENTS.md`

- Add closure gate script reference under "Governance Gates" section

## Files to modify

| File | Action |
|---|---|
| `scripts/ci/close-issue-check.sh` | CREATE — core script |
| `docs/policies/issue-closure.md` | MODIFY — add script ref, labels, update checklist |
| `.claude/skills/ship-gates/SKILL.md` | MODIFY — wire in script at Step 5 and Quick Reference |
| `AGENTS.md` | MODIFY — add closure gate to Governance Gates |

## Verification

1. Run `bash scripts/ci/close-issue-check.sh` from repo root — should produce advisory report
2. Run `bash scripts/ci/close-issue-check.sh --strict` — should exit 1 if any issues found
3. Run `bash scripts/ci/close-issue-check.sh --markdown` — should produce pasteable markdown
4. Run individual flags (`--tests`, `--docs`, `--placeholders`, `--risk`) — each runs only its check
5. Run `bash scripts/ci/check-doc-commands.sh` — should pass (new script path referenced in docs exists)
6. Run `make test-standard` for overall repo health
