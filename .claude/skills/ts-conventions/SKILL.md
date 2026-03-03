---
name: ts-conventions
description: TypeScript coding standards for Shatter. Use when writing or reviewing TypeScript code in shatter-ts/.
user-invocable: true
---

## Strict Mode
- `strict: true` in tsconfig — no exceptions
- `noUncheckedIndexAccess: true` — guard all index access
- **No `any` type** — use `unknown` and narrow, or define types
- No `@ts-ignore` without a linked issue

## Testing
- Jest test runner (`npm test` in `shatter-ts/`)
- Test files: `foo.test.ts` alongside `foo.ts`
- Behavior-descriptive names: `it('rejects malformed protocol messages')`
- Round-trip tests for protocol: serialize → deserialize → verify
- Every public function has tests

## Module System
- Node16 module resolution
- Named exports only; avoid default exports
- `import type` for type-only imports

## Protocol Handlers
- JSON over stdio (newline-delimited)
- Validate and parse into typed discriminated unions
- Match on message type for dispatch
- Report errors via protocol error messages, not thrown exceptions

## Type Safety
- Protocol types mirror `shatter-core/src/protocol.rs`
- Use discriminated unions for message types
- Define types explicitly; don't infer from external data
- `readonly` on properties that shouldn't change

## Code Style
- Short, focused functions
- Precise names that don't need explaining comments
- JSDoc on exported functions — document contracts and non-obvious behavior, not type signatures (see root CLAUDE.md "Inline Documentation")
