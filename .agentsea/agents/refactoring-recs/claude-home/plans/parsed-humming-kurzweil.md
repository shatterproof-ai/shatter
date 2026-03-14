# Plan: Bookmark Fetch SSRF Guardrails (flt-it8.22)

## Context

`PageFetchWorker` in `api/internal/worker/page_fetch.go` fetches user-supplied bookmark URLs with zero SSRF protections. An authenticated user can probe internal services, cloud metadata endpoints (169.254.169.254), or private networks. This is a P0 security bug.

## Approach

Create a new `api/internal/safefetch/` package containing a reusable safe HTTP transport, then wire it into `PageFetchWorker`.

### 1. New package: `api/internal/safefetch/safefetch.go`

Exports:
- `NewTransport(base http.RoundTripper) *Transport` — wraps a base transport with SSRF checks
- `NewClient() *http.Client` — convenience: returns `&http.Client{Transport: NewTransport(http.DefaultTransport), Timeout: 30s}`

The `Transport.RoundTrip` method:
1. **Scheme check**: reject anything other than `http` or `https`
2. **DNS resolution**: resolve hostname via `net.DefaultResolver.LookupIPAddr`, check every returned IP against blocked ranges
3. **IP blocklist**: loopback (127.0.0.0/8, ::1), private (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16), link-local (169.254.0.0/16, fe80::/10), multicast, unspecified (0.0.0.0, ::)
4. **Custom DialContext**: pin the resolved IP so Go's HTTP client uses it (prevents TOCTOU between our check and the actual connection)
5. **Redirect safety**: use `CheckRedirect` on the client to re-validate each redirect target's resolved IP (cap at 10 redirects)
6. **Response size cap**: wrap `resp.Body` with `io.LimitReader` (10 MB default)
7. **Content-Type check**: reject non-text/html content types (optional — may be too strict for some bookmarks; will allow text/*, application/xhtml+xml, application/xml)

### 2. Modify `api/internal/worker/page_fetch.go`

- Change `httpClient()` to return `safefetch.NewClient()` when `w.Client` is nil (the test-injected client path stays as-is)
- Add `maxResponseSize` constant (10 MB)
- Wrap `resp.Body` with `io.LimitReader(resp.Body, maxResponseSize)` in `fetchAndExtract`

### 3. Tests: `api/internal/safefetch/safefetch_test.go`

Comprehensive adversarial test cases:
- **Blocked schemes**: `file:///etc/passwd`, `gopher://`, `ftp://`, `javascript:`
- **Loopback**: `http://127.0.0.1`, `http://[::1]`, `http://localhost`
- **Private ranges**: `http://10.0.0.1`, `http://172.16.0.1`, `http://192.168.1.1`
- **Link-local**: `http://169.254.169.254` (AWS metadata)
- **Redirect to internal**: httptest server that 302s to `http://127.0.0.1`
- **Oversized response**: httptest server returning >10 MB body (verify truncation)
- **Valid external URL**: httptest server on non-blocked address (should succeed)
- **DNS resolution of private IP**: test with custom resolver mock

### 4. Update existing tests in `page_fetch_test.go`

- `TestPageFetchWorker_ConnectionError` currently uses `http://localhost:1` — keep it but note it now gets blocked by SSRF check (update expected error)

## Files to modify/create

| File | Action |
|---|---|
| `api/internal/safefetch/safefetch.go` | Create — SSRF-safe transport |
| `api/internal/safefetch/safefetch_test.go` | Create — adversarial tests |
| `api/internal/worker/page_fetch.go` | Modify — use safefetch, add response size limit |
| `api/internal/worker/page_fetch_test.go` | Modify — update ConnectionError test |

## Verification

```bash
cd api && go test ./internal/safefetch/ -v -count=1
cd api && go test ./internal/worker/ -v -count=1 -short
make api-test-unit && make api-lint   # quality gate
```
