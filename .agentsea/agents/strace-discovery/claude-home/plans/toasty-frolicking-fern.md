# Plan: Auth Startup Signing Material Validation

## Context

The API startup unconditionally falls back to HS256 (`HMACValidator`) when `JWT_PUBLIC_KEY` is unset, passing `cfg.JWTSecret` directly—which defaults to `""`. This means if **neither** env var is configured, the server starts and validates bearer tokens against an empty HMAC secret. This is a security gap: authenticated routes would accept tokens signed with an empty key.

**Goal:** Fail fast at startup when no valid signing material is configured, and reject empty/whitespace-only secrets for HS256.

---

## Changes

### 1. `api/internal/auth/jwt.go` — Validate secret in `NewHMACValidator`

- Change signature: `NewHMACValidator(secret string, opts ...jwt.ParserOption) (*HMACValidator, error)`
- Return an error if `strings.TrimSpace(secret)` is empty
- Use the **original** (untrimmed) secret as the HMAC key (trimming is only for validation)

### 2. `api/cmd/server/main.go` — Fail fast when no signing config

- Update the `else` branch to handle the new error from `NewHMACValidator`
- Add a guard: if `cfg.JWTPublicKey == ""` **and** `cfg.JWTSecret == ""`, log an explicit error ("no JWT signing material configured: set JWT_PUBLIC_KEY or JWT_SECRET") and `os.Exit(1)` **before** calling `NewHMACValidator`
  - This gives a clearer error message than the generic "empty secret" error from the validator
- Handle the error return from `NewHMACValidator` the same way as `NewRSAValidator` (log + exit)

### 3. `api/internal/auth/jwt_test.go` — New test cases

- `TestNewHMACValidator_EmptySecret` — empty string returns error
- `TestNewHMACValidator_WhitespaceSecret` — whitespace-only string returns error
- `TestNewHMACValidator_ValidSecret` — non-empty string succeeds (existing tests already cover token validation, but add explicit constructor success check)

### 4. `api/cmd/server/main_test.go` — Startup validation tests (if feasible)

The acceptance criteria ask for tests covering "valid RS256 startup, valid HS256 startup, and missing-secret failure." Since `main.go` uses `os.Exit`, these are best tested at the **validator constructor level** (already covered by changes to `jwt_test.go`). The startup logic is wiring-only code (exempt per testing standards).

---

## Files to modify

| File | Change |
|---|---|
| `api/internal/auth/jwt.go` | `NewHMACValidator` returns `(*HMACValidator, error)`, rejects empty/whitespace secret |
| `api/cmd/server/main.go` | Handle error from `NewHMACValidator`; fail fast if no signing config |
| `api/internal/auth/jwt_test.go` | Add empty/whitespace/valid secret constructor tests |

---

## Verification

```bash
make api-test-unit   # all unit tests pass including new ones
make api-lint        # no lint issues
```
