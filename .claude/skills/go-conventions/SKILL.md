---
name: go-conventions
description: Go coding standards for Shatter. Use when writing or reviewing Go code in shatter-go/.
user-invocable: true
---

## Tool-Verified Rules
- `go vet ./...` must pass
- `golangci-lint run` must pass
- Prefer extending `golangci-lint` with existing OSS linters such as `revive`, `goconst`, `forbidigo`, and `depguard` over adding custom scripts
- Treat linter-backed documentation checks as the enforcement path for exported API docs; comments should describe behavior and contracts, not syntax

## Error Handling
- Wrap errors with context using `fmt.Errorf("context: %w", err)` when returning them upward
- Check all errors; rely on `errcheck` and the rest of the `golangci-lint` stack to keep this mechanical
- Use sentinel errors or custom types only when callers need stable matching behavior

## Testing
- Follow the repo-level testing policy in the root `CLAUDE.md`
- Use `rapid` for property tests where invariants and round-trips matter; the Go frontend already uses property testing heavily
- Favor table-driven tests and `t.Run` when they improve coverage and readability, but do not force that shape on every test
- `testify` is acceptable if already in use; otherwise prefer the standard library
- Testdata fixtures should document what the analyzer is expected to detect, not merely the language construct being exercised

## Protocol Handlers
- JSON over stdio (newline-delimited)
- `json.Decoder` for streaming input and `json.Encoder` for output
- Dispatch by message type in the handler layer
- Protocol-visible Go types must stay aligned with the shared protocol contract; verify behavior with the repo's parity and conformance tooling rather than source-level guesswork

## Design Guidance
- Define named constants for defaults, timeouts, error codes, and capability lists; avoid scattering protocol literals through handler logic
- Keep interfaces small and driven by actual call sites
- Use pointer receivers for stateful or mutating methods
- Unexport what you can
