# Update docs for kapow binary rename

## Context
`tools/importer/` was renamed to `tools/kapow/`. The Makefile and `tools/CLAUDE.md` are already correct. Remaining docs need updating.

## Changes

### 1. Root `CLAUDE.md` — add kapow targets to commands table
Add these rows to the commands table:
```
| `make kapow-build` | Build the kapow data pipeline binary |
| `make kapow-test` | Run all kapow tool tests |
| `make kapow-test-unit` | Kapow unit tests only (no DB) |
```

### 2. `README.md` — add kapow to tools section
Add kapow to the tools table (line ~222) and add a usage block after samplesql.

### 3. Verify make targets
- `make kapow-build`
- `make kapow-test-unit`

### 4. Commit and push

## Files to modify
- `CLAUDE.md` (root)
- `README.md`

## Verification
- `make kapow-build` succeeds
- `make kapow-test-unit` succeeds
- No remaining `tools/importer/` references in docs
