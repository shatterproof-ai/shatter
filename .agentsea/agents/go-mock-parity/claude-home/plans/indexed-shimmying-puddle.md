# Plan: Visible Claim Scanner (kapow-n92.4)

## Context

User-facing pages (Home, About) and docs (product-overview.md, README.md) make capability claims that should be backed by shipped product contracts. Currently there's no automated check — someone could add marketing copy for an unimplemented feature without anyone noticing. This scanner bridges visible claims to the contract registry.

## Approach: Claims Manifest + Validation Script

A hand-maintained **claims manifest** (`contracts/visible-claims.json`) maps each user-facing claim to its source file and backing contract IDs. A bash script validates the manifest is correct and fresh.

This is preferable to heuristic claim extraction (fragile, false positives) — the manifest makes claim-to-contract mappings explicit and auditable.

## Files to Create

### 1. `contracts/visible-claims.json` — Claims manifest

Array of claim entries:

```json
[
  {
    "id": "home-hero-tagline",
    "claim": "Find your perfect college",
    "source_file": "web/src/pages/Home.tsx",
    "contract_ids": ["institution-search"],
    "waived": false
  },
  {
    "id": "home-feature-save-compare",
    "claim": "save searches, and compare schools",
    "source_file": "web/src/pages/Home.tsx",
    "contract_ids": ["saved-searches", "college-bookmarks"],
    "waived": true,
    "waiver_reason": "Aspirational — contracts are planned, not shipped"
  }
]
```

Fields: `id` (unique kebab-case), `claim` (substring to grep in source), `source_file` (relative path), `contract_ids` (must exist in product-contracts.json), `waived` (bool), `waiver_reason` (optional).

Full manifest will cover ~12-15 claims from Home.tsx, About.tsx, product-overview.md, and README.md.

### 2. `scripts/scan-visible-claims.sh` — Scanner script

Follows `validate-contracts.sh` patterns exactly (set -euo pipefail, REPO_ROOT, color helpers, summary).

**Dependency**: `jq` only (no python3).

**Checks performed:**
1. Manifest is valid JSON
2. All claim IDs are unique
3. For each claim:
   - Source file exists
   - Claim substring found in source file (`grep -qF`) — catches stale claims
   - Each contract_id exists in `contracts/product-contracts.json`
   - Each referenced contract is `shipped` — ERROR if not (unless `waived: true` → WARN)
4. Summary with error/warning counts; exit 1 if any errors

### 3. `scripts/scan-visible-claims_test.sh` — Test script

Follows `validate-contracts_test.sh` patterns (TMPDIR, run_validator, assert_exit, assert_output_contains).

**Test cases:**
- Valid manifest, all shipped contracts → exit 0
- Stale claim (text not in source file) → exit 1
- Missing source file → exit 1
- Unknown contract ID → exit 1
- Unshipped contract, not waived → exit 1
- Unshipped contract, waived → exit 0 + WARN
- Duplicate claim IDs → exit 1
- Empty manifest → exit 0

### 4. Makefile target

```makefile
scan-visible-claims:
	bash scripts/scan-visible-claims.sh
```

## Key files to reference

- `scripts/validate-contracts.sh` — structure/style template
- `scripts/validate-contracts_test.sh` — test structure template
- `contracts/product-contracts.json` — registry to validate against (7 contracts, 5 shipped + 2 planned)
- `web/src/pages/Home.tsx` — hero tagline + 3 feature cards
- `web/src/pages/About.tsx` — hero + mission + 4 highlight cards
- `docs/specs/product-overview.md` — detailed feature descriptions
- `README.md` — brief project description

## Verification

```bash
# Scanner passes on the real codebase
bash scripts/scan-visible-claims.sh

# Tests pass
bash scripts/scan-visible-claims_test.sh

# Confirm the "Save & Compare" claim is flagged as waived warning
bash scripts/scan-visible-claims.sh 2>&1 | grep -i warn
```
