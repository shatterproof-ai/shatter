# Fix shatter-ts tests referencing deleted example files (str-0ouc)

## Context
Commit af3509c moved standalone TS files from `examples/typescript/src/` to `examples/standalone/ts/` but didn't update test paths in shatter-ts. 6 tests fail with ENOENT.

## Changes

**Two files, one string replacement each:**

1. **`shatter-ts/src/handlers.test.ts`** (5 occurrences): Replace `examples/typescript/src` → `examples/standalone/ts`
2. **`shatter-ts/src/executor.test.ts`** (1 occurrence in `EXAMPLES_DIR`): Replace `examples/typescript/src` → `examples/standalone/ts`

The relative path depth (`../../`) stays the same.

## Verification
1. Set up worktree on branch `str-0ouc`
2. Apply edits
3. Run `cd shatter-ts && npm test` — all 6 previously-failing tests should pass
