---
name: check-all
description: Run all language quality gates (Rust, TypeScript, Go) and report a unified summary. Use before committing or after cross-language changes.
allowed-tools: Bash
disable-model-invocation: true
---

Run all three language checks, capture and examine the output from each for errors:

## Rust
1. `cargo test` in workspace root — capture output
2. `cargo clippy -- -D warnings` — capture output

## TypeScript
1. `npm test` in `shatter-ts/` — capture output
2. `npx tsc --noEmit` in `shatter-ts/` — capture output

## Go
1. `go test ./...` in `shatter-go/` — capture output
2. `go vet ./...` in `shatter-go/` — capture output

## Examine & Report

Examine all outputs for failures, warnings, and errors. Report a unified summary:

```
| Language   | Tests | Lint/Vet | Status |
|------------|-------|----------|--------|
| Rust       | ...   | ...      | PASS/FAIL |
| TypeScript | ...   | ...      | PASS/FAIL |
| Go         | ...   | ...      | PASS/FAIL |

Overall: PASS/FAIL
```

Include error details and suggested corrections for any failures.
