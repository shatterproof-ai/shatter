# Plan: Required Check Resolver (kapow-n92.2)

## Context

The diff change classifier (`scripts/classify-changes.sh`) outputs JSON with detected "surfaces" for a given diff. We need a resolver script that maps those surfaces to the exact verification commands required, so that verify-agent and verify-ci can consume a single machine-readable output instead of duplicating mapping logic.

## Implementation

### 1. Create `scripts/resolve-checks.sh`

A bash script that:
- Reads classifier JSON from stdin or a file argument
- Extracts the `surfaces` array
- Maps each surface to its required commands (per the mapping table below)
- Deduplicates commands and determines the highest scope tier
- Outputs JSON: `{"scope": "quick|standard|full", "commands": [...]}`

**Surface-to-command mapping:**

| Surface | Commands | Scope |
|---|---|---|
| `product-copy` | (contract validation — placeholder) | quick |
| `search-surface` | `make api-test-unit` | standard |
| `frontend-shell` | `cd web && pnpm build && pnpm lint && pnpm test` | standard |
| `api-startup` | `make api-test-unit`, `make api-lint` | standard |
| `auth-policy` | `make api-test-unit`, `make api-lint` | standard |
| `logging-privacy` | `make api-test-unit` | quick |
| `web-performance` | `cd web && pnpm build` | quick |
| `graphql-schema` | `make api-generate`, `make web-schema-sync`, `make test-standard` | standard |
| `database` | `make test-full` | full |
| `contracts` | (contract validation — placeholder) | quick |
| `ci-infra` | `make test-quick` | quick |
| `tools` | `make kapow-test-unit` | quick |

Baseline: `make test-quick` always included when any surface is detected.

**Scope hierarchy:** full > standard > quick. The output scope is the maximum across all matched surfaces.

### 2. Create `scripts/resolve-checks_test.sh`

Test script covering:
- Empty surfaces → baseline only (`make test-quick`, scope=quick)
- Single surface (e.g., `database` → scope=full, includes `make test-full`)
- Multiple surfaces → commands deduplicated, highest scope wins
- Unknown surfaces → treated as baseline only
- No input → error exit

### Files to create
- `scripts/resolve-checks.sh` (new)
- `scripts/resolve-checks_test.sh` (new)

### Verification
1. Run the test script: `bash scripts/resolve-checks_test.sh`
2. Run `make test-quick` (quality gate for scripts-only change)
3. Manual spot-check: pipe classifier output into resolver
