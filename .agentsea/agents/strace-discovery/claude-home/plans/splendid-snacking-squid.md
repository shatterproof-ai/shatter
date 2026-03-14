# PostHog Integration Plan

## Context

Kapow has zero analytics or error tracking. This plan adds PostHog to both the
web frontend and Go API for error tracking, product analytics, and operational
logging — while architecting for session replay, experiments, and feature flags
to be enabled later without structural changes.

## Approach

Follow the existing `supabase.ts` pattern: a library module that gracefully
degrades to no-ops when env vars are missing. Wrap the app with
`PostHogProvider` from `posthog-js/react` so the `usePostHog()` hook and
future feature-flag hooks work throughout the tree.

## New dependencies

- **Frontend**: `posthog-js` (includes `posthog-js/react` — provider, hooks)
- **API**: `github.com/posthog/posthog-go` v1.11+ (error tracking, slog handler)
- **API**: `go.opentelemetry.io/otel` + OTLP HTTP exporter (structured logs)

## Environment variables

| Var | Side | Required | Default |
|---|---|---|---|
| `VITE_POSTHOG_KEY` | Web | Yes (to enable) | — |
| `VITE_POSTHOG_HOST` | Web | No | `https://us.i.posthog.com` |
| `POSTHOG_API_KEY` | API | Yes (to enable) | — |
| `POSTHOG_HOST` | API | No | `https://us.i.posthog.com` |

When the key is absent on either side, the integration is inert (no-op).

## Files to create

### 1. `web/src/lib/posthog.ts` — Client initialization

- Read `VITE_POSTHOG_KEY` / `VITE_POSTHOG_HOST` from `import.meta.env`
- Export `posthogEnabled: boolean`
- If enabled: `posthog.init(key, { api_host, capture_pageview: false, capture_pageleave: true, autocapture: false, respect_dnt: true, persistence: 'localStorage' })`
- Register `window.onerror` / `window.onunhandledrejection` → `posthog.capture('$exception', ...)`
- Export the posthog instance
- If disabled: export a no-op proxy (mirrors supabase.ts pattern)

### 2. `web/src/hooks/usePostHog.ts` — Pageview + identify hooks

- `usePostHogPageview()`: captures `$pageview` on `location.pathname` changes via `useLocation()`
- `usePostHogIdentify()`: subscribes to `useAuthStore`, calls `posthog.identify()` / `posthog.reset()`
- Both are no-ops when `posthogEnabled` is false

### 3. `web/src/components/ErrorBoundary.tsx` — React error boundary

- Class component, `componentDidCatch` → `posthog.capture('$exception', ...)`
- Fallback UI using Mantine components (title, message, reload button)

### 4. `web/src/lib/posthogExchange.ts` — urql exchange

- Inspects operation results for `graphQLErrors`
- Captures `graphql_error` events with operation name + error messages
- Passthrough when PostHog is disabled

## Files to modify

### 5. `web/src/main.tsx` — Provider + error boundary wiring

```
StrictMode > UrqlProvider > AppProviders > PostHogProvider > ErrorBoundary > BrowserRouter > App
```

### 6. `web/src/App.tsx` — Wire hooks

- Call `usePostHogPageview()` and `usePostHogIdentify()` in App component

### 7. `web/src/lib/urqlClient.ts` — Add exchange

- Insert `posthogExchange` into exchanges: `[cacheExchange, posthogExchange, authExchange(...), fetchExchange]`

### 8. `web/src/pages/Search.tsx` — Custom analytics events

- `search_executed` — on search results load (filter count, result count, has_location)
- `result_clicked` — on institution click (unit_id, name)
- `export_csv` / `export_pdf` — on export button clicks

### 9. `.env.example` — Document new env vars

### 10. `web/src/test/setup.ts` — Global mocks

- Mock `posthog-js` and `posthog-js/react` with no-op stubs

## PostHog init config

```typescript
posthog.init(key, {
  api_host: host || 'https://us.i.posthog.com',
  capture_pageview: false,       // manual via hook
  capture_pageleave: true,
  autocapture: false,            // explicit events only
  respect_dnt: true,
  persistence: 'localStorage',
  loaded: (ph) => { if (import.meta.env.DEV) ph.debug() },
})
```

## Server-side: Go API error tracking (.7)

### `api/internal/config/config.go` — New env vars

Add `PostHogAPIKey` and `PostHogHost` fields with `env:"POSTHOG_API_KEY"` /
`env:"POSTHOG_HOST"` tags. `PostHogHost` defaults to `https://us.i.posthog.com`.

### `cmd/server/main.go` — PostHog client + slog handler

After pgxpool connect, before router setup:
1. If `cfg.PostHogAPIKey` is set, create `posthog.NewWithConfig(key, config)`
2. Wrap the default slog handler with `posthog.NewSlogCaptureHandler` —
   captures Warn+ as exceptions with stack traces automatically
3. Use `WithDistinctIDFn` to extract user ID from context when available
4. Wire `posthog.Client` into `router.Deps`
5. `defer client.Close()` for flush on shutdown

### `api/internal/router/router.go` — Deps update

Add `PostHog posthog.Client` (or `posthog.EnqueueClient`) to `Deps` struct.
Resolvers can use it for direct exception capture beyond slog.

## Server-side: OpenTelemetry logs (.8)

### Go OTel SDK setup

- Use `go.opentelemetry.io/otel/sdk/log` + `otlploghttp` exporter
- Configure exporter endpoint: `POSTHOG_HOST + /i/v1/logs`
- Auth via project token header
- Create `LoggerProvider`, set as global

### Instrumentation points

- **GraphQL handler**: log query duration as structured attribute
- **Database queries**: middleware/wrapper to log query execution time
- **HTTP middleware**: log request latency per endpoint

## Issues (kapow-d1i)

| Issue | Title | Deps | Priority |
|---|---|---|---|
| `kapow-d1i.1` | PostHog client module + test mocks | — | P1 |
| `kapow-d1i.2` | React error boundary + PostHog exceptions | .1 | P1 |
| `kapow-d1i.3` | GraphQL error tracking exchange | .1 | P2 |
| `kapow-d1i.4` | Pageview + user identify hooks | .1 | P1 |
| `kapow-d1i.5` | Search page analytics events | .1 | P2 |
| `kapow-d1i.6` | PostHog provider wiring | .2, .3, .4, .5 | P1 |
| `kapow-d1i.7` | Go API PostHog error tracking | — | P1 |
| `kapow-d1i.8` | OpenTelemetry logs to PostHog | .7 | P2 |

**Parallelism**: Frontend (.1→.2-.5→.6) and API (.7→.8) tracks are independent
and can run in parallel.

## What this intentionally skips

- **Session replay** — enable later by removing `disable_session_recording: true`
- **Feature flags / experiments** — work automatically once enabled in PostHog dashboard; `PostHogProvider` is already in the tree
- **Cookie consent** — US-focused tool; `respect_dnt: true` is sufficient
