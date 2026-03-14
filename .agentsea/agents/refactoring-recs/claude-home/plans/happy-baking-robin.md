# flt-it8.32.16 — Secrets and Dependency Scanning

## Context

The CI blueprint (`docs/ci/blueprint.md`) already lists `gitleaks` and `OSV-Scanner` as planned analyzers. `scripts/ci/run-static-analyzers.sh` has three sections (gosec, govulncheck, semgrep) following a consistent graceful-skip pattern. This task adds the remaining two scanner sections and standalone Makefile targets.

## Changes

### 1. Add gitleaks section to `scripts/ci/run-static-analyzers.sh`

Same pattern as existing tools: check `command -v gitleaks`, run `gitleaks detect --source .`, report findings, increment `FINDINGS` on failure. Runs from repo root (no `cd api` needed — gitleaks scans the full repo).

### 2. Add osv-scanner section to `scripts/ci/run-static-analyzers.sh`

Same pattern. Run `osv-scanner scan --lockfile=api/go.sum --lockfile=web/pnpm-lock.yaml`. OSV-Scanner is the better fit here — it's lightweight, dependency-focused, and specifically recommended in the blueprint for `go.mod`/`pnpm-lock.yaml`.

### 3. Add Makefile targets

Add to root `Makefile`:
- `scan-secrets`: runs `gitleaks detect --source .` (or skips with message if not installed)
- `scan-deps`: runs `osv-scanner scan --lockfile=...` (or skips with message if not installed)

### 4. Update `run-static-analyzers.sh` summary message

Update the "no tools installed" message to mention gitleaks and osv-scanner alongside the existing tools.

### 5. Add `docs/ci/scanning.md`

Brief doc covering:
- What gitleaks and osv-scanner do
- Installation instructions (go install / brew / binary download)
- How to run standalone (`make scan-secrets`, `make scan-deps`)
- CI artifact handling (SARIF output flags for both tools)
- Integration with existing `run-static-analyzers.sh` and `run-full.sh`

## Files Modified

- `scripts/ci/run-static-analyzers.sh` — add gitleaks + osv-scanner sections
- `Makefile` — add `scan-secrets` and `scan-deps` targets + `.PHONY` entries
- `docs/ci/scanning.md` — new file, scanning documentation
- `docs/ci/blueprint.md` — update "currently supported" list to include gitleaks and osv-scanner

## Verification

```bash
# Script is valid bash
bash -n scripts/ci/run-static-analyzers.sh

# Makefile targets exist (will skip gracefully if tools not installed)
make scan-secrets
make scan-deps

# Quality gates (no Go/web source changed)
make api-test-unit && make api-lint
```
