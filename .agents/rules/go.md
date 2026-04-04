# Go Conventions

- **Error wrapping**: `fmt.Errorf("package: context: %w", err)` — always include package prefix
- **Logging**: `slog.Error("msg", "key", val, "error", err)` — structured, never `fmt.Println`
- **Constructors**: return `(T, error)`; caller decides how to handle; never panic on bad input
- **Context**: always the first parameter — `func Foo(ctx context.Context, ...)`
- **Package names**: single lowercase word matching the directory name
- **File layout**: exported types first, unexported helpers below
- **Interfaces**: named by behaviour (`Validator`, not `IValidator`); defined where consumed
- **Dependencies**: passed explicitly through structs (e.g., `router.Deps`); no `init()` globals
- **SQL**: always parameterized (`$1`, `$2`); never `fmt.Sprintf` into a query string

## Build Verification

Before marking any Go task done:

```bash
go build ./...
go vet ./...
go test ./...
```

## Linting

Run `golangci-lint run` (includes `gosec`). Zero warnings policy.
