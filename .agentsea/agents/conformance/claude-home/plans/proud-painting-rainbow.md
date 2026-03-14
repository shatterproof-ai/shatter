# Plan: flt-it8.32.14 — Semgrep Policy Pack

## Context

`policy/risk-map.json` references "Semgrep custom rules" as a recommended check for trust_boundary (critical), client_secret_storage (high), and extension_surface (low) risk classes. `docs/policies/trust-boundaries.md` defines boundaries B1-B4 with specific threat patterns. No `policy/semgrep/` directory exists yet. This task creates the initial rule set.

## Files to Create

All under `policy/semgrep/`:

### 1. `ssrf.yml` — SSRF Detection (severity: error, maps to critical)

**What it catches**: Direct HTTP calls to user-controlled URLs without `safefetch.Client`.

Rules:
- `flotsam.ssrf.direct-http-get` — Flag `http.Get(...)` in worker/router code (should use safefetch)
- `flotsam.ssrf.direct-http-post` — Flag `http.Post(...)` in worker/router code
- `flotsam.ssrf.new-request-user-url` — Flag `http.NewRequestWithContext` where URL comes from job args/user input without safefetch validation

**Language**: Go
**Target paths**: `api/internal/worker/`, `api/internal/router/`
**Safe pattern**: `safefetch.Client` usage (the existing SSRF-safe client in `api/internal/safefetch/`)

### 2. `placeholder-routes.yml` — Placeholder Route Detection (severity: warning, maps to medium)

**What it catches**: Incomplete route handlers that could ship as if functional.

Rules:
- `flotsam.placeholder.handler-func` — Flag `placeholderHandler(` calls
- `flotsam.placeholder.not-implemented` — Flag `"not yet implemented"` or `http.StatusNotImplemented` in handlers
- `flotsam.placeholder.todo-in-handler` — Flag `TODO` or `FIXME` comments in route handler files

**Language**: Go
**Target paths**: `api/internal/router/`

### 3. `token-storage.yml` — Token Storage Detection (severity: warning, maps to high)

**What it catches**: Insecure client-side token storage patterns across web, extension, and Android.

Rules:
- `flotsam.token.web-localstorage` — Flag `localStorage.setItem` with token-like keys (informational — current pattern, flag for awareness)
- `flotsam.token.web-sessionstorage` — Flag `sessionStorage` for token storage
- `flotsam.token.extension-storage-sync` — Flag `chrome.storage.sync` for tokens (should use `chrome.storage.local`)
- `flotsam.token.android-plain-sharedprefs` — Flag plain `SharedPreferences` for token storage (should use `EncryptedSharedPreferences`)
- `flotsam.token.android-log-token` — Flag logging of token/jwt values

**Languages**: TypeScript, Kotlin
**Target paths**: `web/src/`, `chrome-extension/src/`, `android/`

### 4. `storage-keys.yml` — Client-Controlled Storage Keys (severity: error, maps to critical)

**What it catches**: S3/storage key construction from user input without ownership validation.

Rules:
- `flotsam.storage.path-traversal-in-key` — Flag `..` in storage key construction
- `flotsam.storage.unvalidated-key-construction` — Flag `fmt.Sprintf` building storage keys with user-supplied filename without `ValidateKeyOwnership`
- `flotsam.storage.download-without-ownership` — Flag `storage.Download` calls without prior `ValidateKeyOwnership` check

**Language**: Go
**Target paths**: `api/internal/storage/`, `api/internal/router/`

### 5. `README.md` — Documentation

Brief docs: rule inventory table, how to run (`semgrep --config policy/semgrep/`), severity mapping to risk-map.json, how to add new rules.

## Implementation Steps

1. `bd update flt-it8.32.14 --status in_progress`
2. Create `policy/semgrep/` directory
3. Write `ssrf.yml` with Semgrep rule syntax (patterns, metavariables, message, severity)
4. Write `placeholder-routes.yml`
5. Write `token-storage.yml`
6. Write `storage-keys.yml`
7. Write `README.md`
8. Validate all YAML files parse correctly (`python3 -c "import yaml; yaml.safe_load(open('file'))"`)
9. Run `make test-standard` for repo health
10. Commit and push

## Verification

- All YAML files are valid
- Each rule has: id, pattern(s), message, severity, languages, metadata
- `make test-standard` passes
- Rules are syntactically valid Semgrep YAML (follows Semgrep rule schema)
