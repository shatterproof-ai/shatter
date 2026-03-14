# str-28vd.5: Go Analysis Tool Rollout

## Context

The repo already has `scripts/quality/check-go.sh` wired to run golangci-lint, staticcheck, and govulncheck as optional tools, plus CI docs (`docs/CI-INTEGRATION.md`) recommending them. The missing piece is the actual **golangci-lint configuration file** — without it, `golangci-lint run` uses defaults which may be too lenient or flag irrelevant issues for this codebase.

## Changes

### 1. Create `.golangci.yml` (repo root)

Configuration file with:
- **Linters enabled**: `govet`, `staticcheck`, `errcheck`, `gosimple`, `ineffassign`, `unused`, `gocritic`, `gofumpt`, `misspell`, `revive`
- **Linters disabled**: `exhaustive` (noisy for this codebase), `wrapcheck` (too strict for internal code)
- **Run settings**: Go 1.23, timeout 3m, `shatter-go/` as the working directory target
- **Issues config**: max-same-issues 0 (show all), exclude generated files if any

File: `shatter-go/.golangci.yml` (placed in the Go module directory so golangci-lint finds it automatically when run from `shatter-go/`)

### 2. Validate config against existing code

Run `cd shatter-go && golangci-lint run ./...` and fix any issues or add targeted exclusions for false positives.

### 3. Update `docs/CI-INTEGRATION.md`

Add a new subsection under "Current Limitations" or as a new section explaining:
- **golangci-lint**: always enable in CI (catches style, bugs, performance issues); config committed at `shatter-go/.golangci.yml`
- **staticcheck**: always enable in CI (advanced static analysis, SA-class checks); overlaps with golangci-lint's staticcheck linter but catches additional issues when run standalone
- **govulncheck**: enable on main-branch protection and nightly (checks known vulnerabilities in dependencies); skip on PRs unless dependency files changed

### 4. No script changes needed

`check-go.sh` already handles all three tools correctly with graceful fallback. No modifications required.

## Files to Create/Modify

| File | Action |
|------|--------|
| `shatter-go/.golangci.yml` | **Create** — linter configuration |
| `docs/CI-INTEGRATION.md` | **Edit** — add when-to-enable guidance for each Go analysis tool |

## Verification

1. `cd shatter-go && golangci-lint run ./...` — must pass clean
2. `./scripts/quality/check-go.sh --golangci-lint` — must pass (or skip gracefully if tool not installed)
3. `go vet ./...` in shatter-go — still passes
4. `go test ./...` in shatter-go — still passes
