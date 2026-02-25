---
name: protocol-sync
description: Validate protocol type consistency across Rust, TypeScript, and Go. Use after modifying protocol types in any language.
allowed-tools: Read, Glob, Grep
disable-model-invocation: true
---

Validate that protocol type definitions are consistent across all three languages.

## Files to Read
- **Rust**: `shatter-core/src/protocol.rs`
- **TypeScript**: `shatter-ts/src/protocol.ts`
- **Go**: `shatter-go/protocol/types.go`
- **JSON Schemas** (if present): `protocol/schemas/` — if present, use these as the canonical reference

## Checks
For each message type defined in any language:
1. The message type exists in all three languages
2. Field names match (camelCase in TS/JSON, snake_case in Rust, PascalCase in Go)
3. Field types are equivalent (`String` ↔ `string` ↔ `string`, `Vec<T>` ↔ `T[]` ↔ `[]T`)
4. No missing or extra fields

## Report Mismatches
Flag any inconsistencies found:
- Missing message types in one or more languages
- Missing or extra fields
- Type mismatches
- Naming convention violations

## Output Format

```
## Protocol Sync Report

### Message Types
- [type]: Rust ✓ | TypeScript ✓ | Go ✓
- [type]: Rust ✓ | TypeScript ✗ (missing field: xyz) | Go ✓

### Summary
N message types checked, M issues found
```
