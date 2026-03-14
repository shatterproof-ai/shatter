# Fix Search Button Visibility (kapow-h2ki)

## Context
The homepage search button is nearly invisible — white text on gray background. The root cause: it uses `variant="accent"` on the custom Button wrapper, which maps to `{ variant: 'filled', color: 'accent' }`. With `primaryShade: 9`, Mantine renders accent shade 9 (`#46380d` — dark muddy brown) as background, making the button hard to see against the gray hero section.

The fix changes the search button to use brand primary (dark blue) and standardizes all buttons app-wide to use the custom `Button` wrapper from `@/components/ui/button` with consistent variant choices.

## Changes

### 1. Fix homepage search button (`web/src/pages/Home.tsx:138`)
- Change `variant="accent"` → `variant="default"` (maps to `filled` + `brand` = dark blue bg, white text)

### 2. Migrate direct Mantine Button imports to custom wrapper
These files import `Button` from `@mantine/core` instead of `@/components/ui/button`:

**`web/src/pages/Search.tsx`** — 6 button instances, all using `variant="default"` or no variant
- Change import from `@mantine/core` to `@/components/ui/button`
- Remove `Button` from the Mantine import destructure
- Map variants: Mantine `variant="default"` already matches custom wrapper's `variant="default"` (filled brand)
- For the "Clear all filters" button, remove inline `style={{ color: 'var(--color-primary)' }}` — the custom wrapper already sets brand color

**`web/src/components/search/LocationInput.tsx`** — 3 buttons
- Change import to custom wrapper
- `variant="subtle"` (Clear) → `variant="ghost"` (maps to subtle + brand)
- No variant (Go button) → `variant="default"` (filled brand)
- `variant="light"` (Use my location) → needs a new variant or use Mantine directly. Since "light" is a secondary action, use `variant="outline"` or add a `light` variant to the wrapper. Best: add `light` variant to wrapper (`{ variant: 'light', color: 'brand' }`).

**`web/src/components/search/InstitutionDetail.tsx`** — 1 button
- Change import to custom wrapper
- `variant="light" color="brand"` → use new `light` variant

**`web/src/components/search/RankingBlendBuilder.tsx`** — 1 button
- Change import to custom wrapper
- `variant="subtle"` → `variant="ghost"` (maps to subtle + brand)

### 3. Add `light` variant to custom Button wrapper (`web/src/components/ui/button.tsx`)
- Add `'light'` to `Variant` type union
- Add mapping: `light: { variant: 'light', color: 'brand' }`

### 4. Handle size incompatibilities
The custom wrapper supports sizes `default | sm | lg | xl | icon`. Some Mantine usages use `compact-xs` and `xs` which aren't in the wrapper's size map.
- Add `xs` to the size map: `xs: 'xs'`
- For `compact-xs` in LocationInput (Clear button): use Mantine's `size="compact-xs"` — need to pass through or add to size map. Best: add `compact-xs` to size map.

### 5. Write button component test (`web/src/components/ui/button.test.tsx`)
- Test each variant renders without crashing
- Test that default variant applies filled + brand
- Test size mapping
- Use `renderWithMantine()` from `src/test/render.tsx`

## Files to modify
- `web/src/components/ui/button.tsx` — add `light` variant, `xs` and `compact-xs` sizes
- `web/src/components/ui/button.test.tsx` — new test file
- `web/src/pages/Home.tsx` — change search button variant
- `web/src/pages/Search.tsx` — migrate to custom wrapper
- `web/src/components/search/LocationInput.tsx` — migrate to custom wrapper
- `web/src/components/search/InstitutionDetail.tsx` — migrate to custom wrapper
- `web/src/components/search/RankingBlendBuilder.tsx` — migrate to custom wrapper

## Verification
1. `cd web && pnpm build` — zero errors
2. `cd web && pnpm lint` — zero warnings
3. `cd web && pnpm test` — all tests pass
4. Visual: homepage search button should be dark blue with white text
