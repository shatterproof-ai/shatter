# Fix HTTPS default port in safefetch

## Context
Bookmark enrichment fails for HTTPS URLs without explicit ports. The SSRF-safe fetch client's `pinnedTransport` always defaults to port 80 when `req.URL.Port()` returns empty string, breaking TLS connections which need port 443.

## Bug location
`api/internal/safefetch/safefetch.go:267-270` — `pinnedTransport` method:
```go
func (c *Client) pinnedTransport(ip net.IP, port string) *http.Transport {
    if port == "" {
        port = "80"  // BUG: should be 443 for HTTPS
    }
```

Called at line 164: `c.pinnedTransport(ip, req.URL.Port())` — no scheme info passed.

## Fix

1. **Change `pinnedTransport` signature** to accept scheme: `pinnedTransport(ip net.IP, port, scheme string)`
2. **Default port by scheme**: `"443"` for `"https"`, `"80"` otherwise
3. **Update call site** (line 164) to pass `req.URL.Scheme`

## Tests to add
- `TestPinnedTransportDefaultPort` — unit test verifying `pinnedTransport` returns correct default port for http vs https schemes
- `TestHTTPSURLDefaultPort` — test that `Do` with an HTTPS URL (no explicit port) dials port 443

## Files modified
- `api/internal/safefetch/safefetch.go`
- `api/internal/safefetch/safefetch_test.go`

## Verification
```bash
make api-test-unit && make api-lint
```
