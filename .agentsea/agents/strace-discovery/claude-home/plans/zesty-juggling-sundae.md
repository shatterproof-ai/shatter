# Fix Android upload error path (flt-ghi)

## Context

The Android voice capture has two bugs:
1. **`GraphQLClient.captureVoiceNote()` swallows non-auth GraphQL errors** — lines 76-90 check for auth error codes but silently catch and ignore all other GraphQL errors (the `catch (_: Exception)` at line 88 swallows them).
2. **`RecordScreen.stopAndUpload()` treats any non-exception response as success** — line 145 shows "Voice note captured!" regardless of whether the GraphQL response contains error data, since `captureVoiceNote()` returns a raw string that is never parsed.

## Changes

### 1. Fix `GraphQLClient.kt` — surface all GraphQL errors

**File:** `android/app/src/main/java/com/flotsam/capture/network/GraphQLClient.kt`

Add a new exception class `GraphQLException` (extends `IOException`) for non-auth GraphQL errors.

In the error-checking block (lines 73-90):
- After checking for auth error codes, collect all non-auth error messages
- Throw `GraphQLException` with the joined error messages (mirrors CLI client pattern from `api/internal/cli/client.go:153`)
- Remove the blanket `catch (_: Exception)` that swallows errors
- Keep `catch (e: AuthenticationException) { throw e }` for re-throwing auth errors

Also add a `data class CaptureResult(val id: String, val title: String, val status: String)` and change the return type to parse and return structured data instead of a raw JSON string, so the caller can verify success.

### 2. Add `GraphQLException.kt`

**File:** `android/app/src/main/java/com/flotsam/capture/network/GraphQLException.kt`

Simple exception class extending `IOException`, parallel to `AuthenticationException`.

### 3. Fix `RecordScreen.kt` — no change needed beyond type safety

The current code at line 143 already discards the return value of `captureVoiceNote()`. Once `GraphQLClient` properly throws on error responses, the existing `catch (e: Exception)` block at line 147 will correctly catch `GraphQLException` and display the error message via snackbar.

No UI changes required — the existing error display path (`"Error: ${e.message}"`) will now fire for GraphQL errors that were previously swallowed.

### 4. Add unit tests

**File:** `android/app/src/test/java/com/flotsam/capture/network/GraphQLClientTest.kt`

Add test dependencies to `build.gradle.kts`: JUnit5, MockWebServer (OkHttp).

Tests using `MockWebServer` to feed canned responses:
- `captureVoiceNote returns result on success` — HTTP 200 with valid data
- `captureVoiceNote throws AuthenticationException on 401` — HTTP 401
- `captureVoiceNote throws AuthenticationException on UNAUTHENTICATED error code` — HTTP 200 with auth error in extensions
- `captureVoiceNote throws GraphQLException on non-auth error` — HTTP 200 with validation/server error
- `captureVoiceNote throws IOException on empty body` — HTTP 200, null body

## Verification

1. Add test deps and run: `cd android && ./gradlew test` (if gradlew exists) or verify compilation with `./gradlew assembleDebug`
2. All new tests pass
3. Happy path: success response → returns `CaptureResult`, no exception
4. Error path: non-auth GraphQL error → throws `GraphQLException` with message
5. Auth path: still throws `AuthenticationException` (no regression)

## Files to modify
- `android/app/build.gradle.kts` — add test dependencies
- `android/app/src/main/java/com/flotsam/capture/network/GraphQLClient.kt` — fix error handling
- `android/app/src/main/java/com/flotsam/capture/network/GraphQLException.kt` — new file
- `android/app/src/test/java/com/flotsam/capture/network/GraphQLClientTest.kt` — new tests
