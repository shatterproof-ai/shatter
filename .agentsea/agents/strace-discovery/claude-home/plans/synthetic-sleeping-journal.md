# Plan: kapow-e01 — Namematch Tier 2 (Fuzzy Matching)

## Context

Tier 1 name matching (exact, normalized, alias) in `tools/kapow/internal/namematch/` can't resolve misspelled or variant institution names from external data sources. Tier 2 adds Jaro-Winkler fuzzy matching as a fallback for unmatched names.

## Files to create

### `tools/kapow/internal/namematch/fuzzy.go`

Types:
- `Candidate` struct: `Name string`, `UnitID string`, `State string`
- `FuzzyResult` struct: `Candidate`, `Score float64`
- `UnmatchedEntry` struct: `Name string`, `StateHint string`, `TopCandidates []FuzzyResult`
- `FuzzyMatcher` struct: holds `[]Candidate`, indexed by state (`map[string][]Candidate`)

Functions:
- `NewFuzzyMatcher(candidates []Candidate) *FuzzyMatcher` — builds state index
- `Match(name string, stateHint string) []FuzzyResult` — normalize input with existing `Normalize()`, pre-filter by state if hint provided, compute Jaro-Winkler similarity against candidates' normalized names, return top 5 above 0.92 threshold sorted by score desc
- `WriteReport(path string, unmatched []UnmatchedEntry) error` — JSON marshal to file

Jaro-Winkler: implement directly (~50 lines) to avoid adding a dependency for one function. Standard algorithm with prefix scaling factor p=0.1, max prefix length 4.

### `tools/kapow/internal/namematch/fuzzy_test.go`

Table-driven tests:
1. Misspelled name matches correct institution (e.g., "Harvrd University" → "Harvard University" with high score)
2. State hint filters candidates (fewer results when state provided)
3. Below-threshold scores excluded (dissimilar name returns empty)
4. WriteReport produces valid JSON file
5. Empty candidates returns empty results

## Verification

```bash
cd tools/kapow && go test ./internal/namematch/ -short -race -v && go vet ./internal/namematch/
```
