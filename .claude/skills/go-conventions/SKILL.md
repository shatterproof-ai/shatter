---
name: go-conventions
description: Go coding standards for Shatter. Use when writing or reviewing Go code in shatter-go/.
user-invocable: true
---

## Code Quality
- `go vet ./...` must pass
- `golangci-lint run` must pass
- Follow effective Go and Go proverbs
- `gofmt`/`goimports` for formatting
- Godoc on exported symbols — document behavior and contracts, not syntax (see root CLAUDE.md "Inline Documentation")
- Testdata fixtures document what the analyzer should detect, not what language construct they use

## Error Handling
- Return `error` as last return value
- Wrap with `fmt.Errorf("context: %w", err)`
- Check all errors — no ignored return values
- Sentinel errors or custom types for matchable errors

## Testing
- **Table-driven tests** for all test functions
- Files: `foo_test.go` alongside `foo.go`
- Names: `TestHandler_RejectsUnknownMessageType`
- Use `t.Run` for subtests
- `testify` acceptable if already in use; otherwise stdlib

## Protocol Handlers
- JSON over stdio (newline-delimited)
- Types in `protocol/types.go` mirror `shatter-core/src/protocol.rs`
- `json.Decoder` for streaming input, `json.Encoder` for output
- Dispatch by message type in handler

## Constants
- Define named constants for default values, timeouts, error codes, and capability lists
- Constants file per package: `protocol/constants.go`, `instrument/constants.go`
- Error codes: `const ErrInvalidRequest = "invalid_request"` — never bare strings in handler logic
- Capability lists: define as a package-level slice variable
- Tests reference constants, never duplicate the literal value

## Module Layout
- Module root: `shatter-go/` (`go.mod`)
- Packages: `protocol/` (types + handler), `instrument/` (AST, recorder, sym extraction)
- One responsibility per package
- Unexport what you can

## Struct & Interface
- Accept interfaces, return structs
- Small interfaces (1-3 methods)
- Pointer receivers for state-modifying methods
