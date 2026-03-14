# Plan: Diff Change Classifier (kapow-n92.1)

## Context
Build a script that maps changed files to risk surfaces, enabling future CI optimization by selecting only relevant test suites based on what changed.

## Implementation

### 1. Create `scripts/classify-changes.sh`

Follow existing script conventions (shebang, `set -euo pipefail`, colors, REPO_ROOT).

**Input**: Changed file paths via stdin or `git diff --name-only` args.

**Output**: JSON object mapping surfaces to matched files:
```json
{
  "surfaces": ["search-surface", "api-startup"],
  "details": {
    "search-surface": ["api/internal/search/registry.go"],
    "api-startup": ["api/cmd/server/main.go"]
  }
}
```

**Surface definitions** (pattern → surface mapping):

| Surface | File patterns |
|---------|--------------|
| `product-copy` | `README.md`, `docs/specs/product-overview.md`, `web/src/pages/HomePage*`, `web/src/pages/AboutPage*` |
| `search-surface` | `api/internal/search/*`, `web/src/**/*[Ss]earch*`, `web/src/**/*[Ff]ilter*` |
| `frontend-shell` | `web/src/main.tsx`, `web/src/App.tsx`, `web/src/theme.ts`, `web/src/bootstrap*` |
| `api-startup` | `api/cmd/server/*`, `api/internal/router/*`, `api/internal/config/*` |
| `auth-policy` | `api/internal/auth/*`, `api/internal/middleware/auth*`, `web/src/**/*[Aa]uth*` |
| `logging-privacy` | `api/internal/middleware/logging*` |
| `web-performance` | `web/vite.config.ts`, `web/package.json` |
| `graphql-schema` | `api/graph/schema/*.graphql` |
| `database` | `api/migrations/*`, `common/*` |
| `contracts` | `contracts/*` |

**Approach**: Use bash `case` or pattern matching with `fnmatch`-style globs. Each file is checked against all surface patterns; a file can match multiple surfaces.

**Flags**:
- `--base <ref>`: Compare against git ref (default: `HEAD~1`)
- `--surfaces-only`: Output just the surface names array (no details)
- `--help`: Usage info

### 2. Create `scripts/classify-changes_test.sh`

Test script that:
- Feeds known file lists and asserts correct surface classification
- Tests multi-surface matches (a file matching multiple surfaces)
- Tests unknown files (should produce empty surfaces)
- Tests all defined surfaces have at least one test case
- Uses simple assertion helpers (pass/fail with color output)

### 3. Files to create/modify

- **Create**: `scripts/classify-changes.sh`
- **Create**: `scripts/classify-changes_test.sh`

### Verification

1. Run `./scripts/classify-changes_test.sh` — all tests pass
2. Run `make test-quick` — no regressions
3. Manual test: `echo "api/internal/search/registry.go" | ./scripts/classify-changes.sh` → shows `search-surface`
