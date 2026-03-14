# Plan: OpenTelemetry Logs to PostHog (kapow-d1i.8)

## Context

The Kapow API already has OTel traces/metrics infrastructure (`api/internal/otel/otel.go`) and PostHog error tracking via slog wrapping (`api/internal/posthog/posthog.go`). This issue adds OTel **Logs SDK** to send structured logs to PostHog's OTLP logs endpoint (`https://us.i.posthog.com/i/v1/logs`), with key operational metrics (GraphQL query duration, DB query duration, request latency) as structured log attributes.

The existing slog-based logging already captures `duration_ms` in the HTTP middleware and search resolver — the OTel logs bridge will automatically forward these structured logs to PostHog.

## Approach

Use the `otelslog` bridge (`go.opentelemetry.io/contrib/bridges/otelslog`) to connect Go's `slog` to the OTel Logs SDK, with an OTLP/HTTP exporter configured for PostHog's endpoint. This is the simplest approach because:
- All existing `slog.InfoContext`/`slog.DebugContext` calls (with `duration_ms`, `filter_fields`, etc.) automatically become OTel log records
- No need to rewrite logging call sites
- Standard OTel SDK handles batching, retry, and graceful shutdown

## Implementation Steps

### 1. Add new Go dependencies
```
go.opentelemetry.io/otel/sdk/log
go.opentelemetry.io/otel/exporters/otlp/otlploghttp
go.opentelemetry.io/contrib/bridges/otelslog
```

### 2. Extend `api/internal/otel/otel.go`
- Add `PostHogAPIKey` and `PostHogLogsEndpoint` to `Config` struct
- Create a `LoggerProvider` with OTLP/HTTP exporter configured for PostHog:
  - Endpoint: `us.i.posthog.com` (no path — set path via `WithURLPath`)
  - URL path: `/i/v1/logs`
  - Header: `Authorization: Bearer <POSTHOG_API_KEY>`
- Return `*sdklog.LoggerProvider` from `Init()` alongside existing meter/tracer providers
- Add log provider shutdown to `Shutdown()`
- Only create log provider when `PostHogAPIKey` is non-empty (independent of `OTelEnabled` for traces/metrics)

### 3. Add `api/internal/otel/slogbridge.go`
- Export a `NewSlogHandler(lp)` function that creates an `otelslog.Handler` from the LoggerProvider
- Returns `nil` when `lp` is nil (disabled case)

### 4. Wire in `api/cmd/server/main.go`
- Pass `PostHogAPIKey` to `kapowotel.Config`
- Get `lp` (LoggerProvider) back from `kapowotel.Init()`
- If `lp != nil`, create an otelslog handler and chain it into the slog handler stack:
  - Current chain: `logHandler → PostHog slog wrapper → slog.SetDefault`
  - New chain: `logHandler → PostHog slog wrapper → OTel slog fanout → slog.SetDefault`
  - Use a `slogmulti`-style fanout or a simple custom handler that writes to both the existing handler and the otelslog handler
- Add `lp` to `Shutdown()` defer

### 5. Create `api/internal/otel/fanout.go`
- Simple `FanoutHandler` that sends each log record to multiple `slog.Handler`s
- This avoids adding an external dependency for a trivial adapter

### 6. Write tests — `api/internal/otel/otel_test.go`
- Test `Init()` with logs enabled (PostHogAPIKey set) — verify LoggerProvider is non-nil
- Test `Init()` with logs disabled (empty key) — verify LoggerProvider is nil
- Test `FanoutHandler` dispatches to multiple handlers
- Test `Shutdown()` is nil-safe for all providers

### 7. Update `.env.example`
- POSTHOG_API_KEY is already documented; no new env vars needed
- The logs endpoint is hardcoded (PostHog's standard OTLP path)

## Files to Modify

| File | Change |
|---|---|
| `api/internal/otel/otel.go` | Add LoggerProvider with OTLP/HTTP log exporter for PostHog |
| `api/internal/otel/fanout.go` | **New** — slog fanout handler |
| `api/internal/otel/slogbridge.go` | **New** — otelslog bridge wrapper |
| `api/internal/otel/otel_test.go` | **New** — unit tests |
| `api/internal/otel/fanout_test.go` | **New** — fanout handler tests |
| `api/cmd/server/main.go` | Wire LoggerProvider + slog bridge into startup |
| `api/go.mod` / `api/go.sum` | New OTel log SDK dependencies |

## What's NOT needed (already exists)

- `duration_ms` attribute in HTTP middleware (`middleware/logging.go:39`) — already logged via `slog.InfoContext`
- `duration_ms` attribute in search resolver (`search.resolvers.go:188`) — already logged via `slog.DebugContext`
- `POSTHOG_API_KEY` config field (`config.go:39`) — already defined
- PostHog host config (`config.go:40`) — already defined

## Graceful Degradation

- When `POSTHOG_API_KEY` is empty: no LoggerProvider created, no OTLP exporter, no network calls — pure no-op
- When `OTEL_ENABLED` is false but `POSTHOG_API_KEY` is set: traces/metrics disabled but logs still sent (they're independent)
- On OTLP export failure: OTel SDK handles retries with exponential backoff; logs are dropped after retry exhaustion (no crash)

## Verification

```bash
# Unit tests pass
make api-test-unit

# Linter passes
make api-lint

# Build succeeds
cd api && go build ./...
```
