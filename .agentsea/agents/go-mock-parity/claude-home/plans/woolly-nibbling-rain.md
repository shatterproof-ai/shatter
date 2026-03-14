# flt-it8.32.10: Docs Command Audit

## Context
Documentation files reference make targets, script paths, and bd commands. Some references are stale or inaccurate. This task creates an automated audit script and fixes existing discrepancies.

## Audit Findings

### Broken references to fix
1. **CLAUDE.md** — `make api-build` described as "Compile `bin/flotsam-api`" but actual binary names are `flotsamd`, `flotsam-worker`, `flotsam`. Fix description.
2. **docs/ci/blueprint.md**, **docs/plans/claude-governance-hardening.md** — reference scripts that are planned but not yet implemented (`run-changed.sh`, `run-full.sh`, `run-static-analyzers.sh`, `install-hooks.sh`, `check-doc-commands.sh`). These are in *plan* docs describing future work, so they're acceptable — the audit script should distinguish operator docs from plan docs.
3. **.githooks/pre-commit** and **.githooks/pre-push** — reference `scripts/ci/run-changed.sh` and `scripts/ci/run-full.sh` which don't exist. These hooks will fail if installed. However, fixing hooks is out of scope for this issue.

### Make target coverage (all valid)
All `make` targets referenced in CLAUDE.md, README.md, AGENTS.md, api/CLAUDE.md, and web/CLAUDE.md exist in the corresponding Makefiles. No missing targets.

## Implementation Plan

### 1. Create `scripts/ci/check-doc-commands.sh`
The script will:
- **Scan operator docs** (CLAUDE.md, README.md, AGENTS.md, api/CLAUDE.md, web/CLAUDE.md) — NOT plan docs
- Extract `make <target>` references, resolve which Makefile they belong to (root vs api/ vs web/ based on doc context)
- Verify each target exists in the appropriate Makefile
- Extract `scripts/...` path references from operator docs
- Verify each referenced script file exists on disk
- Report mismatches with file:line detail
- Exit 0 if clean, exit 1 if mismatches found

### 2. Fix CLAUDE.md stale description
- Change "Compile `bin/flotsam-api`" → "Build all API binaries (flotsamd, flotsam-worker, flotsam)"

### Key files
- Create: `scripts/ci/check-doc-commands.sh`
- Edit: `CLAUDE.md` (fix api-build description)

## Verification
```bash
cd /home/ketan/project/flotsam/.claude/worktrees/worktree/docs-audit
bash scripts/ci/check-doc-commands.sh   # must exit 0
```
