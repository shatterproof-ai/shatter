# Plan: Fix enum code→label display (kapow-amiy)

## Context

Institution type and other enum fields (locale, region, Carnegie Basic, NCAA division, etc.) are stored as numeric/text codes in the database (e.g. "1" for Public) but displayed as raw codes in the table view and PDF export. The detail card (`InstitutionDetail.tsx`) already has correct hardcoded maps, but they are duplicated and isolated — not shared with the table or export. The goal is to extract those maps into a single shared utility and apply it everywhere (table, detail, PDF export).

## Root Cause

- `helpers.ts::formatCellValue()` only handles DOLLARS/PERCENTAGE/MILES — no enum mapping
- `ResultsTable.tsx` uses `formatCellValue(getFieldValue(inst, field.outputKey), field.unit)` — raw code shown
- `pdfExport.ts` uses the same `getCellValue()` via `formatCellValue` — raw code in PDF
- `InstitutionDetail.tsx` has correct mappings but they are local constants, not shared

## Audit: All fields needing mapping

From `api/internal/search/registry.go` and `InstitutionDetail.tsx`:

| Field name | Codes → Labels |
|---|---|
| `institution_type` | 1→Public, 2→Private nonprofit, 3→Private for-profit |
| `highest_degree` | 1→Certificate, 2→Associate's, 3→Bachelor's, 4→Master's, 5→Doctorate |
| `locale` | 11→City: Large, 12→City: Midsize, ... 43→Rural: Remote (12 values) |
| `region` | 0→U.S. Service Schools, 1→New England, ... 9→Outlying Areas |
| `ncaa_division` | 1→Division I, 2→Division II, 3→Division III |
| `koppen_climate` | A→Tropical, B→Dry, C→Temperate, D→Continental, E→Polar |
| `partisan_direction` | D→Democrat, R→Republican |
| `online_only` | 0→No, 1→Yes (stored in JSONB, may not need mapping) |
| `gender` | coed→Coed, men-only→Men Only, women-only→Women Only (already labels in source) |

## Implementation Plan

### Step 1: Create `web/src/lib/enumLabels.ts`

New file — single shared utility:

```ts
/** All code→label maps keyed by search field name */
export const ENUM_LABELS: Record<string, Record<string, string>> = {
  institution_type: { '1': 'Public', '2': 'Private nonprofit', '3': 'Private for-profit' },
  highest_degree:   { '1': 'Certificate', '2': "Associate's", '3': "Bachelor's", '4': "Master's", '5': 'Doctorate' },
  locale:           { '11': 'City: Large', /* ... 12 values ... */ },
  region:           { '0': 'U.S. Service Schools', '1': 'New England', /* ... */ },
  ncaa_division:    { '1': 'Division I', '2': 'Division II', '3': 'Division III' },
  koppen_climate:   { A: 'Tropical', B: 'Dry', C: 'Temperate', D: 'Continental', E: 'Polar' },
  partisan_direction: { D: 'Democrat', R: 'Republican' },
}

/** Return human-readable label for a code, or null if field/code not found */
export function lookupEnumLabel(fieldName: string, code: string | null | undefined): string | null {
  if (code == null) return null
  return ENUM_LABELS[fieldName]?.[code] ?? null
}
```

### Step 2: Update `web/src/components/search/helpers.ts`

Extend `formatCellValue` signature to accept optional `fieldName`:

```ts
// Import at top
import { lookupEnumLabel } from '@/lib/enumLabels'

export function formatCellValue(
  value: unknown,
  unit?: SearchFieldUnit | null,
  fieldName?: string,
): string {
  if (value === null || value === undefined || value === '') return '—'

  // Check enum label first (before numeric formatting)
  if (fieldName) {
    const label = lookupEnumLabel(fieldName, String(value))
    if (label !== null) return label
  }

  const num = Number(value)
  if (unit === 'DOLLARS' && !isNaN(num)) { ... }  // unchanged
  // ... rest unchanged
}
```

This is backward-compatible (new param is optional).

### Step 3: Update `web/src/components/search/ResultsTable.tsx`

Pass `field.name` to `formatCellValue`:

```tsx
// Line ~269 — change:
{formatCellValue(getFieldValue(inst, field.outputKey), field.unit)}
// to:
{formatCellValue(getFieldValue(inst, field.outputKey), field.unit, field.name)}
```

### Step 4: Update `web/src/components/search/pdfExport.ts`

In `getCellValue()`, pass `field.name`:

```ts
// Line ~18 — change:
return formatCellValue(raw, field.unit)
// to:
return formatCellValue(raw, field.unit, field.name)
```

### Step 5: Refactor `web/src/components/search/InstitutionDetail.tsx`

- Remove all local `const INSTITUTION_TYPES = ...`, `HIGHEST_DEGREES`, etc.
- Import `ENUM_LABELS` from `@/lib/enumLabels`
- Change `mapLabel(value, INSTITUTION_TYPES)` → `mapLabel(value, ENUM_LABELS.institution_type)`
- The `mapLabel` function itself stays local (it's a pure formatting helper, not a shared mapping)

### Step 6: Write tests `web/src/lib/enumLabels.test.ts`

```ts
import { describe, it, expect } from 'vitest'
import { lookupEnumLabel, ENUM_LABELS } from './enumLabels'

describe('lookupEnumLabel', () => {
  it('maps institution type codes to labels', () => {
    expect(lookupEnumLabel('institution_type', '1')).toBe('Public')
    expect(lookupEnumLabel('institution_type', '2')).toBe('Private nonprofit')
    expect(lookupEnumLabel('institution_type', '3')).toBe('Private for-profit')
  })
  it('returns null for unknown field', () => {
    expect(lookupEnumLabel('unknown_field', '1')).toBeNull()
  })
  it('returns null for null/undefined code', () => {
    expect(lookupEnumLabel('institution_type', null)).toBeNull()
    expect(lookupEnumLabel('institution_type', undefined)).toBeNull()
  })
  it('returns null for unknown code (fallback to raw)', () => {
    expect(lookupEnumLabel('institution_type', '99')).toBeNull()
  })
  it('maps locale codes', () => {
    expect(lookupEnumLabel('locale', '11')).toBe('City: Large')
    expect(lookupEnumLabel('locale', '43')).toBe('Rural: Remote')
  })
  // ... etc for all maps
})
```

## Critical Files

| File | Change |
|---|---|
| `web/src/lib/enumLabels.ts` | **NEW** — single shared mapping utility |
| `web/src/lib/enumLabels.test.ts` | **NEW** — tests for utility |
| `web/src/components/search/helpers.ts` | Add `fieldName?` param to `formatCellValue` |
| `web/src/components/search/ResultsTable.tsx` | Pass `field.name` to `formatCellValue` |
| `web/src/components/search/pdfExport.ts` | Pass `field.name` to `formatCellValue` |
| `web/src/components/search/InstitutionDetail.tsx` | Import from shared utility, remove local maps |

## Verification

```bash
cd web
pnpm test        # enumLabels.test.ts must pass, no regressions
pnpm build       # tsc + vite build — zero errors
pnpm lint        # zero warnings

# From worktree root:
make test-standard
```

Manual check: in table view, institution type column should show "Public", "Private nonprofit", "Private for-profit" instead of "1", "2", "3".
