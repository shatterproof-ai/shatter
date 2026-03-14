# PostHog Client Module + Test Mocks (kapow-d1i.1)

## Context
Kapow needs analytics/error tracking. This issue sets up the PostHog JS client module as the foundation for the full integration (kapow-d1i epic).

## Steps

### 1. Install posthog-js
```bash
cd web && pnpm add posthog-js
```

### 2. Create `web/src/lib/posthog.ts`
Follow the `supabase.ts` pattern:
- Read `VITE_POSTHOG_KEY` and `VITE_POSTHOG_HOST` from `import.meta.env`
- Export `posthogEnabled: boolean` (true when key is present)
- If enabled: call `posthog.init()` with config from plan (capture_pageview: false, autocapture: false, respect_dnt: true, persistence: 'localStorage', etc.)
- If disabled: log a console.info and export `posthog` as-is (posthog-js already no-ops before init)
- Export the posthog instance

### 3. Add global mocks in `web/src/test/setup.ts`
Mock both `posthog-js` and `posthog-js/react` with vi.mock():
- `posthog-js`: init, capture, identify, reset, debug, register, opt_out_capturing — all vi.fn()
- `posthog-js/react`: PostHogProvider (passthrough), usePostHog (returns mock)

### 4. Document env vars in `.env.example`
Add `VITE_POSTHOG_KEY` and `VITE_POSTHOG_HOST` with comments under a new PostHog section.

### 5. Write unit tests `web/src/lib/posthog.test.ts`
Mirror `supabase.test.ts` pattern with vi.resetModules + vi.stubEnv:
- Test: exports posthogEnabled=false and doesn't call init when key missing
- Test: exports posthogEnabled=true and calls init with correct config when key present
- Test: uses default host when VITE_POSTHOG_HOST is absent
- Test: uses custom host when VITE_POSTHOG_HOST is set

### 6. Quality gate
```bash
cd web && pnpm build && pnpm lint && pnpm test
```

## Key files
- Pattern to follow: `web/src/lib/supabase.ts`, `web/src/lib/supabase.test.ts`
- Test setup: `web/src/test/setup.ts`
- Env docs: `.env.example`

## Verification
- `pnpm build` passes (posthog-js types resolve)
- `pnpm lint` passes (zero warnings)
- `pnpm test` passes (new tests + existing tests unaffected by mocks)
