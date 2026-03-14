# PostHog Provider Wiring

## Context
The PostHog JS SDK is already initialized in `web/src/lib/posthog.ts` and analytics events are captured in Search.tsx and other places. However, the `PostHogProvider` from `posthog-js/react` is not wired into the React component tree. Adding it enables React hooks like `usePostHog()` from the library and follows PostHog's recommended React integration pattern.

## Plan

### 1. Edit `web/src/main.tsx`
- Import `PostHogProvider` from `posthog-js/react`
- Import `posthog` instance from `@/lib/posthog`
- Insert `<PostHogProvider client={posthog}>` between `AppProviders` and `ErrorBoundary` per the specified hierarchy

Target hierarchy:
```
StrictMode > UrqlProvider > AppProviders > PostHogProvider > ErrorBoundary > BrowserRouter > App
```

### 2. Verify
```bash
cd web && pnpm install && pnpm build && pnpm lint && pnpm test
```

## Files to modify
- `web/src/main.tsx` — add PostHogProvider wrapping

## Existing code to reuse
- `web/src/lib/posthog.ts` — exports `posthog` instance (already initialized)
- `posthog-js` package already in `package.json` — `posthog-js/react` ships with it
