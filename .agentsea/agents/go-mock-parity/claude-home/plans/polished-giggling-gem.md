# Plan: Standard gate repair (flt-o4r)

## Context
`make test-standard` fails in a clean environment because `ext-typecheck` (chrome-extension TypeScript check) hard-fails when `chrome-extension/node_modules` isn't installed. This is the only remaining issue — all other steps (Go tests, web build, web lint, web tests, Go lint) pass cleanly.

## Issues found
1. **`ext-typecheck` hard-fails** without `chrome-extension/node_modules` — unlike `android-lint` which gracefully skips when tooling is missing
2. **CLAUDE.md docs mismatch** — `test-standard` is described as "quick + web unit tests + Go lint" but actually also includes `ext-typecheck`

## Fix

### 1. Make `ext-typecheck` skip gracefully (Makefile line ~118-120)
Follow the same pattern as `android-lint` — check if `chrome-extension/node_modules` exists before running:

```makefile
ext-typecheck:
	@if [ -d chrome-extension/node_modules ]; then \
		cd chrome-extension && pnpm run typecheck; \
	else \
		echo "chrome-extension/node_modules not found — run 'make ext-install' first (skipping)"; \
	fi
```

### 2. Update CLAUDE.md docs
Add `ext-typecheck` to the `test-standard` description in the Commands table.

## Verification
- `make test-standard` passes without pre-installing chrome-extension deps
- `make ext-install && make test-standard` also passes (typecheck actually runs)
