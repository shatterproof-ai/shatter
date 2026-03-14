# Plan: Enrollment/Cost Min-Max Dropdown Selects

## Context

Enrollment and cost range filters currently use free-text number inputs. Users must type exact numbers, which is slow and error-prone. Replacing these with dropdown selects using predefined increment steps improves usability — users pick from sensible breakpoints instead of guessing values.

## Approach

### 1. Create step configuration module

**New file**: `web/src/components/search/rangeSteps.ts`

Define well-named constants for the dropdown options:

```ts
// Enrollment: 0, 1000, 2000, ... 10000 (by 1,000), then 12500, 15000, ... 50000 (by 2,500)
// Cost: 0, 5000, 10000, ... 30000 (by $5,000), then 40000, 50000, ... 80000 (by $10,000)
```

Exported functions:
- `generateEnrollmentSteps(): number[]` — returns the enrollment breakpoints
- `generateCostSteps(): number[]` — returns the cost breakpoints
- `formatStepLabel(value: number | null, unit: 'enrollment' | 'cost'): string` — formats with commas, `$` prefix for cost, `"No limit"` for null
- `getStepOptions(fieldName: string): SelectOption[] | null` — returns Mantine Select options for fields that should use dropdowns, or `null` for fields that keep text inputs

**Field name mapping** (which fields get dropdowns):
- `enrollment`, `grad_enrollment` → enrollment steps
- `tuition_in_state`, `tuition_out_of_state`, `avg_net_price`, `avg_net_price_0_30k`, `endowment` → cost steps
- Identification: match on `field.name` in the component

The `"No limit"` option uses `null` value (represented as empty string `""` in the Select) for unbounded min/max.

### 2. Modify FilterControl.tsx — NumericalRangeSearchField branch

**File**: `web/src/components/search/FilterControl.tsx` (lines 495–584)

In the `NumericalRangeSearchField` block:
1. Import `getStepOptions` from `rangeSteps.ts`
2. Call `getStepOptions(field.name)` to check if this field uses dropdowns
3. If options exist → render two Mantine `<Select>` components (Min / Max) instead of `<DebouncedNumberInput>`
4. If no options → keep existing text input behavior (no change for percentage fields, etc.)

The Select components:
- Min select: options from first step up to the selected max (or all), plus "No minimum" placeholder
- Max select: options from selected min (or first) to last step, plus "No maximum" placeholder
- Both include "No limit" / unlimited as the unbounded option
- `onChange` converts selected string value back to number and calls `handleRangeChange`
- Clear button remains for resetting both

### 3. Add unit tests

**New file**: `web/src/components/search/rangeSteps.test.ts`

Test cases:
- `generateEnrollmentSteps()` returns correct sequence and length
- `generateCostSteps()` returns correct sequence and length
- `formatStepLabel()` formats enrollment numbers with commas (e.g., "12,500")
- `formatStepLabel()` formats cost with `$` prefix (e.g., "$5,000")
- `getStepOptions()` returns options for enrollment/cost field names
- `getStepOptions()` returns null for non-dropdown fields

## Files to modify

| File | Change |
|---|---|
| `web/src/components/search/rangeSteps.ts` | **NEW** — step generation, formatting, option building |
| `web/src/components/search/rangeSteps.test.ts` | **NEW** — unit tests for step logic |
| `web/src/components/search/FilterControl.tsx` | Conditionally render `<Select>` instead of `<DebouncedNumberInput>` for enrollment/cost fields |

## Key decisions

- **Identify fields by `field.name`**, not by `field.unit` — `unit` is `null` for enrollment fields and some cost fields share `DOLLARS` with non-dropdown fields
- **No API changes needed** — the filter value format (`{ min?: number, max?: number }`) stays the same
- **No URL serialization changes** — dropdown values are plain numbers, same as text input
- **Mantine Select** — already a project dependency, consistent with existing UI patterns

## Verification

```bash
cd web && pnpm build && pnpm lint && pnpm test
```

- Build must pass with zero errors
- Lint must pass with zero warnings
- All existing + new tests must pass
