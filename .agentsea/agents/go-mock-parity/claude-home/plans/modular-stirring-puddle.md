# Plan: flt-it8.18 — CLI Client (cross-platform)

## Context

Flotsam needs a cross-platform CLI binary (`flotsam`) for capturing notes/bookmarks, searching, and listing items from the terminal. The CLI lives in the same Go module but communicates with the API exclusively via HTTP/GraphQL — no server internal imports. The existing `api/cmd/flotsam/main.go` is a stub printing "not yet implemented".

## CLI Framework

**`urfave/cli/v3`** — already an indirect dependency in `go.mod` (pulled in by River). Zero new transitive deps. Lighter than cobra, supports subcommands and flags natively.

## Package Structure

```
api/cmd/flotsam/main.go          -- Command tree wiring (urfave/cli/v3)
api/internal/cli/config.go       -- Config file management (~/.config/flotsam/)
api/internal/cli/config_test.go
api/internal/cli/client.go       -- GraphQL HTTP client with auth
api/internal/cli/client_test.go  -- httptest-based tests
api/internal/cli/format.go       -- Tabwriter output formatting
api/internal/cli/format_test.go
```

## Command Tree

```
flotsam
├── login [--email --password]   -- authenticate, store JWT
├── config set-url <url>         -- set API URL
├── capture <text> [--title] [--tags] [--sensitivity]  -- create note
├── bookmark <url> [--title] [--tags] [--sensitivity]  -- create bookmark
├── search <query> [--limit] [--type]                  -- search items
├── list [--type] [--limit] [--sort]                   -- list items
├── voice                        -- stub: "not yet implemented"
└── version                      -- print version/build info
```

## Implementation Steps

### Step 1: `internal/cli/config.go` + tests
- `Config` struct: `APIURL string`, `Token string` (JSON serialized)
- `ConfigDir()` — `$XDG_CONFIG_HOME/flotsam/` or `~/.config/flotsam/`, creates dir 0700
- `Load()` — read config.json, defaults if missing (APIURL: `http://localhost:8081`)
- `Save(*Config)` — write config.json with 0600 permissions
- `SetURL(url)`, `SetToken(token)` — load/update/save helpers
- Env overrides: `FLOTSAM_API_URL`, `FLOTSAM_TOKEN` take precedence over file
- Tests: temp dirs, roundtrip, env overrides, missing file defaults

### Step 2: `internal/cli/client.go` + tests
- `Client` struct with `httpClient`, `apiURL`, `token`
- `NewClient(apiURL, token string) *Client`
- `Do(ctx, query, variables, result)` — generic GraphQL POST to `/graphql`, auth header, error extraction
- Typed methods: `Login()`, `CaptureNote()`, `CaptureBookmark()`, `Search()`, `ListItems()`
- Local response types (Item, SearchResult, etc.) — NOT imported from `graph/model/`
- GraphQL queries as string constants
- Tests: `httptest.NewServer` mocking, verify auth header, request shape, error handling

### Step 3: `internal/cli/format.go` + tests
- `FormatItem(w, Item)` — single item display
- `FormatItemTable(w, []Item)` — tabwriter table (ID[8], TYPE, TITLE, TAGS, CREATED)
- `FormatSearchResult(w, SearchResult)` — table + total/hasMore footer
- `Truncate(s, max)` — truncate long strings
- Uses stdlib `text/tabwriter`

### Step 4: `cmd/flotsam/main.go`
- Promote `urfave/cli/v3` to direct dep
- Build `cli.App` with all subcommands
- Each command: load config → create client → call method → format output
- `login`: accept `--email`/`--password` flags, or prompt interactively (use `x/term` for password echo suppression)
- `voice`: print stub message, exit 0
- `version`: print from `internal/version` package

### Step 5: Quality gate
```bash
make api-test-unit && make api-lint
```

## Key Design Decisions

1. **No server internal imports** — CLI only uses HTTP. Standalone distributable binary.
2. **Env var overrides** — `FLOTSAM_API_URL` / `FLOTSAM_TOKEN` for CI/scripting.
3. **Interactive password** — `x/term.ReadPassword()` for echo suppression; `--password` flag for scripts.
4. **Single `internal/cli` package** — simple enough for 3 files, split later if needed.
5. **Human-readable output only** — `--json` can be added later.

## Files Modified
- `api/cmd/flotsam/main.go` — replace stub with full CLI
- `api/internal/cli/config.go` — new
- `api/internal/cli/config_test.go` — new
- `api/internal/cli/client.go` — new
- `api/internal/cli/client_test.go` — new
- `api/internal/cli/format.go` — new
- `api/internal/cli/format_test.go` — new
- `api/go.mod` / `api/go.sum` — promote urfave/cli/v3 to direct

## Verification
1. `make api-test-unit` — all tests pass, >= 80% coverage on `internal/cli`
2. `make api-lint` — zero issues
3. `go build ./cmd/flotsam` — binary compiles
