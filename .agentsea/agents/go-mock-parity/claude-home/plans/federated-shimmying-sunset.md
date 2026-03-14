# Plan: Contract Traceability Checker (kapow-n92.6)

## Context

Protected source surfaces (GraphQL schema, search registry, pages, config, product overview) can change without corresponding contract/doc updates, breaking traceability. This script detects that gap.

## Implementation

Create `scripts/check-contract-traceability.sh` following the pattern of `check-config-parity.sh` (colored output, summary, exit codes).

### Script structure

1. **Header/colors** — same BOLD/GREEN/RED/RESET pattern as `check-config-parity.sh`
2. **Parse `--base` flag** — default `main`, used as git diff base
3. **Define protected surfaces** as associative array mapping glob patterns to descriptions:
   - `api/graph/schema/*.graphql` → "GraphQL schema"
   - `api/internal/search/registry.go` → "Search field registry"
   - `web/src/pages/*.tsx` → "Frontend pages"
   - `api/internal/config/config.go` → "Config env vars"
   - `docs/specs/product-overview.md` → "Product claims"
4. **Get changed files** via `git diff --name-only $BASE...HEAD` (CI) or `git diff --name-only $BASE` (local)
5. **Match changed files** against protected patterns
6. **For each matched file**, check if the diff also includes changes to:
   - Any `contracts/*.json` file
   - Any `docs/specs/*.md` file
   - OR the file is listed in `scripts/traceability-waivers.txt`
7. **Report** with colored output per file, summary at end
8. **Exit 1** if any untraced changes found, **exit 0** otherwise

### Waiver file

Create `scripts/traceability-waivers.txt` with header comments explaining format. Each line is a file path that is waived from traceability requirements. Empty to start.

### Key files
- **Create**: `scripts/check-contract-traceability.sh`
- **Create**: `scripts/traceability-waivers.txt`
- **Reference**: `scripts/check-config-parity.sh` (pattern to follow)

## Verification

```bash
cd /home/ketan/project/kapow/.claude/worktrees/worktree/traceability
bash scripts/check-contract-traceability.sh --base main
# Should exit 0 (no changes on fresh branch) or report any untraced changes
```

Then make a test change to a protected file and verify it catches it.
