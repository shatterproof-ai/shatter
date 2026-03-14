# Plan: flt-it8.32.5 — Generic CI Entrypoints

## Context
The repo has risk classification (`detect-risk.py`), doc-command auditing (`check-doc-commands.sh`), and placeholder checking (`check-placeholders.py`), but no top-level CI entrypoint scripts. The `.githooks/pre-commit` and `.githooks/pre-push` already exist (in the main repo, untracked) and call `scripts/ci/run-changed.sh --staged` and `scripts/ci/run-full.sh` respectively. We need to create these scripts and add Makefile targets.

## Deliverables

### 1. `scripts/ci/run-changed.sh`
Runs risk-aware checks on changed files (diff against origin/main, or `--staged`).

```
#!/usr/bin/env bash
set -euo pipefail

# Pass --staged to operate on staged changes (for pre-commit hook)
# Default: diff against origin/main

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

MODE_ARGS=()
[[ "${1:-}" == "--staged" ]] && MODE_ARGS=(--staged)

STRICT="${FLOTSAM_STRICT:-0}"
EXIT_CODE=0

# Step 1: Risk classification
echo "=== Risk Classification ==="
if ! python3 scripts/ci/detect-risk.py "${MODE_ARGS[@]}" --severity-threshold critical; then
    echo "CRITICAL risk detected — review required"
    [[ "$STRICT" == "1" ]] && EXIT_CODE=1
fi

# Step 2: Determine what changed (Go? Web? Docs?)
if --staged: git diff --cached --name-only; else git diff --name-only origin/main...HEAD
Parse file lists to detect go_changed, web_changed, docs_changed

# Step 3: Run targeted checks based on what changed
- Go changed → make api-test-unit api-lint
- Web changed → make web-build web-lint
- Docs changed → bash scripts/ci/check-doc-commands.sh && python3 scripts/ci/check-placeholders.py

# Step 4: Advisory vs strict exit
```

**Detailed logic:**
- Detect changed files using git diff (staged or vs origin/main)
- Categorize: `api/**` → Go, `web/**` → Web, docs files → Docs
- Run only the relevant subset of checks
- Always run `detect-risk.py` for classification/reporting
- Always run `check-doc-commands.sh` if docs changed
- Always run `check-placeholders.py` if docs changed
- Advisory mode (default): print warnings, exit 0
- Strict mode (`FLOTSAM_STRICT=1`): exit non-zero on failures

### 2. `scripts/ci/run-full.sh`
Full-repo checks: `make test-standard` + static analyzers + policy checks.

```
#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

STRICT="${FLOTSAM_STRICT:-0}"
EXIT_CODE=0

# Step 1: Risk classification (full repo)
echo "=== Risk Classification ==="
python3 scripts/ci/detect-risk.py --all || true

# Step 2: make test-standard (Go unit + web build + lint + web test + Go lint)
echo "=== Test Standard ==="
make test-standard || { echo "test-standard FAILED"; EXIT_CODE=1; }

# Step 3: Static analyzers
echo "=== Static Analyzers ==="
bash scripts/ci/run-static-analyzers.sh || { echo "Static analysis findings"; [[ "$STRICT" == "1" ]] && EXIT_CODE=1; }

# Step 4: Policy checks
echo "=== Policy Checks ==="
bash scripts/ci/check-doc-commands.sh || { echo "Doc command mismatches"; [[ "$STRICT" == "1" ]] && EXIT_CODE=1; }
python3 scripts/ci/check-placeholders.py || { echo "Placeholder mismatches"; [[ "$STRICT" == "1" ]] && EXIT_CODE=1; }

exit $EXIT_CODE
```

### 3. `scripts/ci/run-static-analyzers.sh`
Run available analyzers, skip gracefully if not installed.

```
#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

FINDINGS=0

# gosec
if command -v gosec &>/dev/null; then
    echo "=== gosec ==="
    (cd api && gosec ./...) || FINDINGS=1
else
    echo "gosec not installed — skipping"
fi

# govulncheck
if command -v govulncheck &>/dev/null; then
    echo "=== govulncheck ==="
    (cd api && govulncheck ./...) || FINDINGS=1
else
    echo "govulncheck not installed — skipping"
fi

# semgrep
if command -v semgrep &>/dev/null; then
    echo "=== semgrep ==="
    semgrep --config auto . || FINDINGS=1
else
    echo "semgrep not installed — skipping"
fi

exit $FINDINGS
```

### 4. `scripts/install-hooks.sh`
```
#!/usr/bin/env bash
set -euo pipefail
repo_root="$(git rev-parse --show-toplevel)"
git config core.hooksPath "$repo_root/.githooks"
echo "Git hooks configured: .githooks/"
```

### 5. `.githooks/pre-commit` and `.githooks/pre-push`
Already exist in the main repo (untracked). Copy them into the worktree as-is:
- `pre-commit`: calls `scripts/ci/run-changed.sh --staged`
- `pre-push`: calls `scripts/ci/run-full.sh`

### 6. Makefile targets
Add to root Makefile:
```makefile
## ci-changed: Run risk-aware checks on changed files (vs origin/main)
ci-changed:
	bash scripts/ci/run-changed.sh

## ci-full: Run full-repo policy and analyzer checks
ci-full:
	bash scripts/ci/run-full.sh

## hooks-install: Configure git to use .githooks/
hooks-install:
	bash scripts/install-hooks.sh
```

Add these to the `.PHONY` declaration.

## Files to Create/Modify
- **Create**: `scripts/ci/run-changed.sh` (new)
- **Create**: `scripts/ci/run-full.sh` (new)
- **Create**: `scripts/ci/run-static-analyzers.sh` (new)
- **Create**: `scripts/install-hooks.sh` (new)
- **Create**: `.githooks/pre-commit` (copy from main repo)
- **Create**: `.githooks/pre-push` (copy from main repo)
- **Modify**: `Makefile` — add ci-changed, ci-full, hooks-install targets + .PHONY

## Verification
1. `bash scripts/ci/run-changed.sh` — self-test (should detect our own changes)
2. `bash scripts/ci/run-full.sh` — full-repo test (runs test-standard + analyzers)
3. `bash scripts/install-hooks.sh` — verify hooks path set
4. `make ci-changed` and `make ci-full` — verify Makefile targets work
5. `make test-standard` — overall repo health
