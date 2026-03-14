# Plan: str-28vd.1 — CI Integration Docs

## Context

The CI integration doc (`docs/CI-INTEGRATION.md`) already exists on main as an untracked file with substantial content covering script inventory, stage layout, CI modes, tool expectations, hook/agent guidance, and current limitations. The `scripts/quality/` directory has 8 scripts plus a `lib/common.sh`.

The task is to formalize this doc on a branch, verify accuracy against actual scripts, and fill any gaps per the acceptance criteria.

## Assessment

The existing doc already covers all acceptance criteria:
- PR pipeline with scripts and fail criteria ✓
- Main-branch pipeline ✓
- Scheduled/nightly jobs ✓
- Agent/hook invocation of same scripts ✓
- Strict mode vs permissive mode (partially — needs a dedicated section)

## Changes

### File: `docs/CI-INTEGRATION.md`

1. **Copy from main** — bring the existing untracked file into the worktree branch
2. **Add a dedicated "Strict vs Permissive Mode" section** — currently scattered across multiple sections; consolidate into one clear explanation of `--strict-optional` behavior (skip vs fail on missing tools)
3. **Verify script flags** match actual implementations (check-all.sh accepts `--strict-optional` and `--e2e`, confirmed)
4. **Minor polish** — ensure the doc references `scripts/quality/lib/common.sh` as shared infrastructure, and that all 8 scripts are listed

No other files need changes — this is a docs-only task.

## Verification

- `cat docs/CI-INTEGRATION.md` exists and is well-formed Markdown
- Content references all `scripts/quality/` entrypoints
- No hardcoded absolute paths
- Commit prefixed with `str-28vd.1:`
- Push branch
