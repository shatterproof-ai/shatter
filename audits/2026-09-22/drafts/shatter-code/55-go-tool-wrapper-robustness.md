# shatter-go-tool: non-atomic extract can cache a truncated binary; no HTTP timeouts; GitHub API call on every run; tests cover only arg parsing

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | go,distribution,installer,audit |
| parent | audit epic (draft 00) |
| blocked by | draft 51 |
| related | str-fl9g.2 |
| source findings | frontend-go-10 |

<!-- body -->
## Problem

The `go tool shatter` wrapper can permanently cache a partial binary and can hang or rate-limit on network access.

## Current code facts / evidence

- `shatter-go-tool/cmd/shatter/main.go:340-348` extractBinary: OpenFile(targetBinary, O_TRUNC, 0o755) + io.Copy directly; `isExecutable` (:352) checks mode bits only; SHA covers the archive only.
- `main.go:141-147` `latestContinuousBuild` runs before the cache check (:160) on every invocation when SHATTER_BUILD is unset (unauthenticated limit 60/h).
- `main.go:267` http.DefaultClient.Do and `:279` http.Get: no timeout; API request reads GITHUB_TOKEN (:263), downloadFile does not.
- `main_test.go` is 39 lines (parseArgs only).

## Acceptance criteria

- Extract to a temp file in targetDir, chmod, os.Rename (same pattern as shatter-go/launcher/launcher.go:336-350).
- http.Client with timeouts; resolved latest tag cached with a TTL; offline falls back to newest cached build.
- httptest-backed tests: manifest selection, checksum mismatch, partial-extract recovery.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: frontend-go-10 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-fl9g.2
