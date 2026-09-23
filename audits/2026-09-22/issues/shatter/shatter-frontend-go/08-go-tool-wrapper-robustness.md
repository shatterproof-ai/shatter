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

- **Non-atomic extract.** The binary is extracted straight into its final cache path with mode 0755. An interrupted extract or two concurrent first runs leave a truncated executable; the cache check only tests mode bits, so every later run execs the corrupt file. The SHA-256 check covers the archive, not the extracted binary.
- **No timeouts.** Both HTTP calls use the default client with no timeout.
- **API call before cache.** With `SHATTER_BUILD` unset, the latest continuous build is resolved via `api.github.com` before the cache is consulted, on every run; unauthenticated users get 60 requests/hour, so CI loops get rate-limited.
- **Token only on the API call.** The API request sends `GITHUB_TOKEN`; the asset download does not (matters for private repos or authenticated rate limits).
- **Tests.** `main_test.go` covers only `parseArgs`.

## Evidence

Re-verified against the audit worktree at commit 56c86168 (`shatter-go-tool/cmd/shatter/main.go`, 355 lines):

- `:315 extractBinary`; `:340` `os.OpenFile(targetBinary, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o755)` then copies directly.
- `:352 isExecutable` checks mode bits only; used as the cache hit test at `:160`.
- `:142` `latestContinuousBuild(repo)` runs before the `:160` cache check; `:195` issues the API request.
- `:256 getJSON` sets the `Authorization` header from `GITHUB_TOKEN` at `:263` and calls `http.DefaultClient.Do` at `:267` (no timeout).
- `:278 downloadFile` calls `http.Get(url)` at `:279` (no timeout, no token).
- `shatter-go-tool/cmd/shatter/main_test.go`: 39 lines, `parseArgs` only.
- Reference pattern for atomic writes: `shatter-go/launcher/launcher.go:336-350` builds to a temp path and `os.Rename`s (`:350`).

## Acceptance criteria

- [ ] Extraction writes to a temp file in the target directory, is fsynced and chmodded, then `os.Rename`d into place. If the release manifest carries a per-binary hash, it is verified before the rename.
- [ ] Both HTTP calls use an `http.Client` with connect and overall timeouts; `downloadFile` sends `GITHUB_TOKEN` when set.
- [ ] The resolved latest-continuous tag is cached with a TTL; when the API is unreachable or rate-limited, the newest cached build is used with a warning.
- [ ] `httptest`-backed tests, each failing on current main where applicable and passing on the branch: manifest selection, checksum mismatch rejected, a truncated cached binary from an interrupted extract is not executed and is replaced, API unavailable falls back to cache, a stalled server times out.
- [ ] `task meta` (which runs `go test ./...` in the wrapper module) passes; `task affected` `Gates selected` recorded.

## Suggested approach

Mirror the launcher's temp-then-rename; wrap the client in a small struct so tests can point it at `httptest.NewServer`. Store the TTL-cached tag in a small JSON file next to the cached binaries.

## Out of scope

- The module path/directory mismatch (`go-tool-module-path`), which lands first and may move this file.

## Dependencies

- Blocked by: `go-tool-module-path` (directory/module rename touches the same files).
- Related: str-fl9g.2 (closed; original wrapper).

## Size

M

## References

- Finding frontend-go-10 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-10). Verifier correction applied: the API request does read `GITHUB_TOKEN`; only the download ignores it. Old draft: `drafts/shatter-code/55-go-tool-wrapper-robustness.md`.
