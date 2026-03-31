---
name: ts-conventions
description: TypeScript coding standards for Shatter. Use when writing or reviewing TypeScript code in shatter-ts/.
user-invocable: true
---

## Tool-Verified Rules
- `strict: true` in `tsconfig`
- `noUncheckedIndexedAccess: true` in `tsconfig`
- Prefer `ESLint` with `typescript-eslint` for enforceable policy: `no-explicit-any`, `ban-ts-comment`, `consistent-type-imports`, `consistent-type-exports`, and no default exports
- Use lint-backed documentation rules for exported APIs if the repo adds them; JSDoc should explain contracts and non-obvious behavior, not repeat types

## Testing
- Jest is the test runner in `shatter-ts/`
- Follow the repo-level testing policy in the root `CLAUDE.md`
- `fast-check` is a primary testing tool here. Add property tests for protocol round-trips, invariants, and malformed input where they matter
- When touching protocol-visible behavior, capabilities, or handler outputs, run the repo's parity and conformance gates instead of relying on local examples alone

## Module System
- Node16 module resolution
- Prefer named exports over default exports
- Use `import type` for type-only imports

## Protocol Handlers
- JSON over stdio (newline-delimited)
- Validate incoming data and parse into typed discriminated unions
- Match on message type for dispatch
- Report errors through protocol error responses, not thrown exceptions that escape the handler boundary

## Type Safety
- Protocol-visible TypeScript types must stay aligned with the shared protocol contract; verify observable behavior with the repo's parity and conformance tooling
- Use discriminated unions for message types
- Define types explicitly; do not infer trusted shapes from external data
- Use `readonly` where immutability prevents accidental drift

## Design Guidance
- Define named constants for defaults, timeouts, and values shared between production and test code
- Keep protocol constants in `src/protocol.ts` and local execution constants with the module that owns them
- Tests should import important shared constants instead of duplicating protocol literals
