# Coverage Policy Rollout (flt-it8.32.13)

## Context

Coverage thresholds are documented in CLAUDE.md (80% per package) but not enforced
anywhere in CI. Web has Vitest coverage configured with thresholds in
`vitest.config.ts`, but `make test-standard` runs `pnpm test` (no coverage), not
`pnpm test:coverage`. Go has no coverage tooling at all. This issue adds a unified
coverage check script, wires it into CI, and documents the policy.

## Deliverables

### 1. `scripts/ci/check-coverage.sh`

Single script that checks both Go and web coverage. Advisory by default,
blocking when `FLOTSAM_STRICT=1`.

**Go coverage:**
- Run `go test -short -coverprofile=coverage.out ./...` from `api/`
- Parse per-package coverage from `go tool cover -func=coverage.out`
- Check each package against 80% threshold
- Exempt packages: `cmd/*`, `internal/router`, `internal/server`, `graph/*`, `internal/testutil`
- Report pass/fail per package with actual percentage

**Web coverage:**
- Run `pnpm test:coverage` from `web/` (Vitest already enforces 80% thresholds
  and exits non-zero on failure — no custom parsing needed)
- Capture exit code

**Output:**
- Clear per-component summary (Go packages + web aggregate)
- Exit 0 in advisory mode (default), exit 1 in strict mode on failures

### 2. Add coverage step to `scripts/ci/run-full.sh`

Insert as Step 5 (after policy checks, before summary):

```bash
# --- Step 5: Coverage checks ---
echo "=== Coverage Checks ==="
if ! bash scripts/ci/check-coverage.sh; then
    echo "Coverage checks reported failures"
    if [[ "$STRICT" == "1" ]]; then
        EXIT_CODE=1
    fi
fi
```

### 3. Root Makefile target

Add `coverage` target that delegates to the script:

```makefile
## coverage: Run coverage checks for Go and web
coverage:
	bash scripts/ci/check-coverage.sh
```

Add to `.PHONY` list.

### 4. `docs/ci/coverage-policy.md`

Document:
- Per-package 80% line coverage threshold (Go and web)
- Exempt Go packages and why (wiring-only, generated code, test utilities)
- Web coverage is enforced by Vitest thresholds in `vitest.config.ts`
- Advisory vs strict mode (`FLOTSAM_STRICT=1`)
- Android and extension coverage explicitly deferred until test bases are credible
- How to run: `make coverage` or as part of `make ci-full`

### 5. CLAUDE.md updates

Add `make coverage` to the Commands table in root CLAUDE.md.

## Files to create/modify

| File | Action |
|---|---|
| `scripts/ci/check-coverage.sh` | **Create** — main coverage check script |
| `scripts/ci/run-full.sh` | **Edit** — add coverage step |
| `Makefile` | **Edit** — add `coverage` target + .PHONY |
| `docs/ci/coverage-policy.md` | **Create** — policy documentation |
| `CLAUDE.md` | **Edit** — add `make coverage` to commands table |

## Verification

```bash
# From worktree root:
make test-standard          # existing tests still pass
bash scripts/ci/check-coverage.sh   # runs and reports (advisory)
FLOTSAM_STRICT=1 bash scripts/ci/check-coverage.sh  # strict mode
bash scripts/ci/run-full.sh  # full CI includes coverage
bash scripts/ci/check-doc-commands.sh  # new make target documented correctly
```
