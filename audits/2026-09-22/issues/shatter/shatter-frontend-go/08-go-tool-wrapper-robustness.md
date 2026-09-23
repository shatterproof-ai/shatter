---
slug: go-tool-wrapper-robustness
kind: new
title: "shatter-go-tool: non-atomic extract can cache a truncated binary; no HTTP timeouts; GitHub API call on every run; tests cover only arg parsing"
priority: P2
type: bug
labels: [go-tool, distribution, installer, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [go-tool-module-path]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# shatter-go-tool: non-atomic extract can cache a truncated binary; no HTTP timeouts; GitHub API call on every run; tests cover only arg parsing

## Problem

The `go tool shatter` wrapper downloads and caches the shatter release binary. It can permanently cache a partial binary, can hang forever on a stalled connection, and hits the GitHub API on every invocation.

- **Non-atomic extract, and a cache check that trusts anything executable.** The binary is extracted straight into its final cache path with mode 0755. An interrupted extract or two concurrent first runs leave a truncated executable. The cache check only tests mode bits, so every later run execs the corrupt file. The SHA-256 check covers the archive, not the extracted binary. Making extraction atomic prevents new corrupt entries but does not repair ones already in users' caches, because the mode-bit check accepts them before any extraction runs.
- **No timeouts.** Both HTTP calls use the default client with no timeout.
- **API call before cache.** With `SHATTER_BUILD` unset, the latest continuous build is resolved via `api.github.com` before the cache is consulted, on every run; unauthenticated users get 60 requests/hour, so CI loops get rate-limited.
- **Token only on the API call.** The API request sends `GITHUB_TOKEN`; the asset download does not. Adding it naively would be unsafe: the download URL comes from the release manifest (`asset.URL`), so it could point at any host.
- **Tests.** `main_test.go` covers only `parseArgs`.

## Evidence

Re-verified against the audit worktree at commit 56c86168 (`shatter-go-tool/cmd/shatter/main.go`, 355 lines):

- `:315 extractBinary`; `:340` `os.OpenFile(targetBinary, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o755)` then copies directly.
- `:352 isExecutable` checks mode bits only; it is the cache-hit test at `:160` (cache dir `os.UserCacheDir()/shatter/binaries/<build>/<platform>`, `:154-158`).
- `:142` `latestContinuousBuild(repo)` runs before the `:160` cache check; `:195` issues the API request.
- `:256 getJSON` sets the `Authorization` header from `GITHUB_TOKEN` at `:263` and calls `http.DefaultClient.Do` at `:267` (no timeout).
- `:278 downloadFile` calls `http.Get(url)` at `:279` (no timeout, no token). Its URL is `selected.URL` (`:180`) from the manifest's `asset.url` field (`:36-41`). `release.yml:262` writes `https://github.com/{repo}/releases/download/{tag}/{name}` there, which GitHub redirects to a separate download host.
- `shatter-go-tool/cmd/shatter/main_test.go`: 39 lines, `parseArgs` only.
- Reference pattern for atomic writes: `shatter-go/launcher/launcher.go:336-350` builds to a temp path and `os.Rename`s (`:350`).

## Acceptance criteria

- [ ] **Atomic extract.** Extraction writes to a temp file in the target directory, is fsynced and chmodded, then `os.Rename`d into place.
- [ ] **Cache validation and migration.** The wrapper writes a sidecar (e.g. `<binary>.sha256`) after the rename, holding the SHA-256 of the extracted binary (and the manifest's per-binary hash when the manifest carries one). A cache hit requires the sidecar to exist and match the binary's current hash. Entries with no sidecar (everything cached by current releases) or a mismatching one are treated as untrusted: deleted and re-downloaded, with a one-line notice. This is documented in `docs/distribution.md`.
- [ ] **Timeouts.** Both HTTP calls use an `http.Client` with connect and overall timeouts.
- [ ] **Token scoping.** `GITHUB_TOKEN` is sent only to an allowlist of origins (`https://api.github.com` and `https://github.com`), on the initial request and on redirects to an allowlisted origin; never to any other host, including redirect targets such as the release-asset download host. Plain `http://` URLs never receive it.
- [ ] **Latest-tag cache.** The resolved latest-continuous tag is cached with a TTL; when the API is unreachable or rate-limited, the newest cached build is used with a warning.
- [ ] `httptest`-backed tests, each failing on current main where applicable and passing on the branch:
  - manifest selection; archive checksum mismatch rejected;
  - a pre-existing truncated executable in the cache with no sidecar (as current releases leave it) is not executed and is replaced;
  - a cached binary whose sidecar hash does not match is not executed and is replaced;
  - API unavailable falls back to the cached tag;
  - a stalled server times out within the configured bound;
  - with `GITHUB_TOKEN` set, a manifest asset URL on a non-allowlisted host receives no `Authorization` header, and a redirect from an allowlisted test origin to a non-allowlisted one does not carry it (make the allowlist injectable so the test server can stand in for github.com).
- [ ] `task meta` (which runs `go test ./...` in the wrapper module) passes; `task affected` passes with `Gates selected` recorded.

## Suggested approach

Mirror the launcher's temp-then-rename. Wrap the client in a small struct holding the timeouts and the token allowlist, with a `CheckRedirect` that removes `Authorization` for non-allowlisted hosts (Go already drops it on cross-domain redirects, but the initial request to a manifest-supplied host is the gap). Store the TTL-cached tag in a small JSON file next to the cached binaries.

## Out of scope

- The module path/directory mismatch (`go-tool-module-path`), which lands first and may move this file.
- Private-repo asset downloads through the API asset endpoint.

## Dependencies

- Blocked by: `go-tool-module-path` (directory/module rename touches the same files).
- Related: str-fl9g.2 (closed; original wrapper).

## Size

M

## References

- Finding frontend-go-10 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-10). Verifier correction applied: the API request does read `GITHUB_TOKEN`; only the download ignores it. Old draft: `drafts/shatter-code/55-go-tool-wrapper-robustness.md`. Revised after the Codex cross-check: cache validation/migration for existing corrupt entries, and origin-scoped token.
