# kapow-j2l.5: Static platform lists (UC, CSU, Florida)

## Context
The platform scraper system needs static extractors for three application platforms (UC Application, Cal State Apply, Florida SUS) whose member lists are fixed and don't require web scraping.

## Files to modify
- `tools/kapow/internal/platform/static.go` — **create**: three `staticExtractor` implementations
- `tools/kapow/internal/platform/platform.go` — **edit**: update `All()` to return static extractors
- `tools/kapow/internal/platform/static_test.go` — **create**: tests

## Implementation

### 1. Create `static.go`
- Define a `staticExtractor` struct with `key`, `label`, and `names []string` fields
- Implement `Key()`, `Label()`, `Extract()` methods (Extract ignores browser, returns names)
- Three constructor functions or package-level vars for UC (9), CSU (23), Florida (12)

### 2. Update `All()` in `platform.go`
- Return slice of the three static extractors

### 3. Create `static_test.go`
- Count assertions (9, 23, 12)
- Spot-check specific names
- Verify non-empty names
- Verify Extract returns no error

## Verification
```bash
cd tools/kapow && go test ./internal/platform/... -short -count=1
```
