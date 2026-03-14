# flt-it8.31: Client Auth Handling Hardening

## Context

All clients (web, Chrome extension, Android, CLI) handle auth errors inconsistently. The web frontend's urql auth exchange checks for `extensions.code === 'UNAUTHENTICATED'` / `'FORBIDDEN'` in GraphQL errors, but the API never returns these codes — resolver auth failures are plain `errors.New("resolver: authentication required")` without extensions. This means the web client's auth error detection only works via the HTTP 401 fallback, and GraphQL-level auth errors (from resolvers behind OptionalAuth) are invisible to the auth exchange.

The Chrome extension and Android client don't distinguish auth errors from other failures at all — users see generic "HTTP 401" or raw error messages.

## Changes

### 1. API: Add GraphQL error codes for auth errors

**File**: `api/graph/resolver/helpers.go`

Replace `errAuthRequired = errors.New(...)` with a helper that returns a `gqlgen`-compatible error with `extensions.code`:

```go
import "github.com/vektah/gqlparser/v2/gqlerror"

func authRequiredError() *gqlerror.Error {
    return &gqlerror.Error{
        Message:    "authentication required",
        Extensions: map[string]any{"code": "UNAUTHENTICATED"},
    }
}
```

Update `requireAuth()` to return `authRequiredError()` instead of `errAuthRequired`.

**File**: `api/graph/resolver/helpers_test.go` — Update test to check the new error type.

### 2. CLI: Parse GraphQL error extensions for auth codes

**File**: `api/internal/cli/client.go`

- Add `Extensions map[string]any` field to `graphqlError` struct
- In the error-joining logic inside `Do()`, check if any error has `extensions.code == "UNAUTHENTICATED"` and return the friendly "not authenticated" message (same as HTTP 401 path)
- This means auth errors surfaced through GraphQL (not just HTTP 401) get the user-friendly message

**File**: `api/internal/cli/client_test.go` — Add test for GraphQL UNAUTHENTICATED extension code.

### 3. Chrome extension: Detect and surface auth errors

**File**: `chrome-extension/src/background/service-worker.ts`

- Add `extensions` to the `GraphQLResponse.errors` type: `Array<{ message: string; extensions?: { code?: string } }>`
- After checking `!res.ok`, add specific 401 detection: return `{ success: false, error: "Authentication failed — check your token in extension settings" }`
- After checking GraphQL errors, detect UNAUTHENTICATED/FORBIDDEN codes and return a specific auth error message

### 4. Android: Detect auth errors with typed exceptions

**File**: `android/app/src/main/java/com/flotsam/capture/network/GraphQLClient.kt`

- Check for HTTP 401 specifically: throw a distinct `AuthenticationException` (new class) instead of generic `IOException`
- Parse the GraphQL response body for errors even on success, checking `extensions.code`

**File**: `android/app/src/main/java/com/flotsam/capture/network/ApiClient.kt`

- Same 401 detection for the upload endpoint

**File**: `android/app/src/main/java/com/flotsam/capture/network/AuthenticationException.kt` (new)

- Simple `class AuthenticationException(message: String) : IOException(message)` so callers can catch it distinctly

### 5. Android: Use EncryptedSharedPreferences for token storage

**File**: `android/app/src/main/java/com/flotsam/capture/data/SettingsStore.kt`

- Current: `preferencesDataStore` (plaintext on disk, app-sandboxed but not encrypted)
- Change to: `EncryptedSharedPreferences` from AndroidX Security library for the JWT token specifically
- This is the Android-recommended approach for storing sensitive credentials

> **NOTE**: This requires adding `androidx.security:security-crypto` to `build.gradle`. Check if it's already a dependency first.

### 6. Web frontend: No code changes needed

The web client already handles this correctly:
- `urqlClient.ts:17-23` checks `extensions.code` for UNAUTHENTICATED/FORBIDDEN
- Falls back to `error.response?.status === 401`
- Calls `logout()` on auth error via `refreshAuth()`

The only gap was that the API wasn't returning the extension codes — fix #1 closes that gap. Once the API returns `extensions.code: "UNAUTHENTICATED"`, the web client's existing detection will work properly.

## Files Modified

| File | Change |
|---|---|
| `api/graph/resolver/helpers.go` | Return `gqlerror.Error` with UNAUTHENTICATED code |
| `api/graph/resolver/helpers_test.go` | Update test for new error type |
| `api/internal/cli/client.go` | Parse error extensions, detect auth codes |
| `api/internal/cli/client_test.go` | Test for UNAUTHENTICATED extension handling |
| `chrome-extension/src/background/service-worker.ts` | Type extensions, detect 401 and auth error codes |
| `android/.../network/GraphQLClient.kt` | 401 detection, parse error extensions |
| `android/.../network/ApiClient.kt` | 401 detection |
| `android/.../network/AuthenticationException.kt` | New exception class |
| `android/.../data/SettingsStore.kt` | EncryptedSharedPreferences for token |

## Verification

1. **Go tests**: `make api-test-unit && make api-lint` — covers resolver helpers and CLI client changes
2. **Web**: `cd web && pnpm build && pnpm lint` — verify no breakage (no web code changes)
3. **Chrome extension**: Manual review (no test infra exists)
4. **Android**: Manual review (no test infra in repo)
5. **Standard gate**: `make test-standard`
