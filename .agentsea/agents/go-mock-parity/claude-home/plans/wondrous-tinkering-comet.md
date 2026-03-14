# flt-it8.32.15 — Go security analyzer gate

## Context

The CI governance flow (`scripts/ci/run-static-analyzers.sh`) already has gosec and govulncheck sections that gracefully skip if tools aren't installed. However, there's no dedicated Makefile target to run just security analyzers, no documentation on installing the tools, and `run-changed.sh` doesn't trigger security analysis when Go files change.

## Changes

### 1. Add `make api-security` target (`api/Makefile` + root `Makefile`)

In `api/Makefile`, add a `security` target that runs gosec + govulncheck directly (not via the shell script), with graceful skip if not installed:

```makefile
security:
	@echo "=== gosec ==="
	@if command -v gosec >/dev/null 2>&1; then gosec ./...; else echo "gosec not installed, skipping"; fi
	@echo "=== govulncheck ==="
	@if command -v govulncheck >/dev/null 2>&1; then govulncheck ./...; else echo "govulncheck not installed, skipping"; fi
```

In root `Makefile`, add `api-security` proxy:
```makefile
api-security:
	$(MAKE) -C api security
```

### 2. Integrate into `run-changed.sh`

When `go_changed=1`, add an optional security analyzer call after lint:
```bash
echo "--- Go security analyzers (advisory) ---"
make api-security || echo "[advisory] security analyzers reported findings"
```

### 3. Add `docs/ci/go-security.md`

Short doc covering:
- What gosec and govulncheck do
- Install commands (`go install` one-liners)
- How to run (`make api-security`)
- CI integration (advisory by default, strict with `FLOTSAM_STRICT=1`)
- Note that gosec also runs via golangci-lint in `make api-lint`

### 4. Update root `CLAUDE.md` commands table

Add `make api-security` row to the commands table.

## Files to modify
- `api/Makefile` — add `security` target
- `Makefile` — add `api-security` proxy target
- `scripts/ci/run-changed.sh` — add security analyzers call when Go changes
- `docs/ci/go-security.md` — new documentation file
- `CLAUDE.md` — add row to commands table

## Verification
```bash
make api-security          # should run (or skip gracefully)
make api-test-unit         # existing tests still pass
make api-lint              # existing lint still passes
```
