# Plan: str-4omc — Language Support Docs Consistency

## Context
README.md lists only TypeScript and Go as supported languages. SPEC.md also lists the Rust frontend as a stub. The project structure in README omits `shatter-rust/`. Users see conflicting information depending on which doc they read.

## Changes

### 1. README.md — Update language support table (lines 46-51)
Add Rust row with "Stub" status. Add a note pointing to SPEC.md as the canonical reference.

```markdown
## Supported Languages

| Language   | Frontend      | Status |
|------------|---------------|--------|
| TypeScript | `shatter-ts`  | Supported |
| Go         | `shatter-go`  | Supported |
| Rust       | `shatter-rust`| Stub (protocol handler only) |

See [SPEC.md §1.3](SPEC.md#13-supported-languages) for the canonical language support matrix including file extensions and implementation details.
```

### 2. README.md — Update project structure (lines 236-243)
Add `shatter-rust/` entry:

```
shatter-rust/     Rust frontend (stub — protocol handler only)
```

### 3. SPEC.md — Add canonical marker to §1.3 (line 48)
Add a note designating this table as the source of truth:

```markdown
### 1.3 Supported Languages

> **Canonical source of truth** for language support status. Other docs link here.
```

### 4. SPEC.md — Update "Last updated" date (line 3)
Change from 2026-02-28 to 2026-03-10.

## Files Modified
- `README.md`
- `SPEC.md`

## Verification
- Visual review of both files for consistency
- No code changes, no tests needed
