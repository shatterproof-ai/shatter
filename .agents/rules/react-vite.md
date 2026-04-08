# React / TypeScript / Vite Conventions

## Styling

Use Mantine style props (`bg`, `c`, `p`, `m`, `fz`, `fw`, etc.) and layout components (`Container`, `Group`, `Stack`, `SimpleGrid`, `Box`). For one-off styles, use the `style` prop. Do **not** use Tailwind utility classes.

**Never hardcode hex color values** — use Mantine color references (`c="brand.9"`, `color="accent"`) or CSS custom properties (`var(--color-primary)`).

PostCSS is configured with `postcss-preset-mantine` and `postcss-simple-vars`.

## Components

- Named exports (not default exports)
- New UI components go in `src/components/ui/<name>.tsx`
- Each component gets a colocated test file (`<name>.test.tsx`)

## Pages

1. Create `src/pages/NewPage.tsx` with a named export
2. Add the route in `src/App.tsx`

## Linting

**Zero warnings policy** — ESLint runs with `--max-warnings 0`. Path alias `@/` maps to `src/`.

## Build Verification (Mandatory)

**Every change to frontend code must be verified with a full build before it is considered done:**

```bash
pnpm build   # tsc -b && vite build — must finish with zero errors
pnpm lint    # eslint --max-warnings 0 — must finish with zero warnings
```

A clean `pnpm build` is the only reliable gate — it catches missing/removed imports, type errors, and broken module resolution that per-file checks miss.

## Testing (Vitest)

- Coverage thresholds enforced at 80%
- New components require a test file alongside them
- Use `renderWithMantine()` from `src/test/render.tsx` when testing components that use Mantine
