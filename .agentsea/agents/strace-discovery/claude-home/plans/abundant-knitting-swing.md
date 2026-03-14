# Plan: Implement tools/kapow/internal/namematch (Tier 1 Deterministic)

## Context

Issue kapow-6vz: a new package for deterministic institution name matching (~95% of
matches). The matcher loads institutions + aliases from PostgreSQL into memory maps and
resolves input names to `unit_id` via a 3-tier lookup hierarchy.

## Files to Create

| File | Purpose |
|------|---------|
| `tools/kapow/internal/namematch/normalize.go` | `Normalize(s string) string` — canonicalize names |
| `tools/kapow/internal/namematch/aliases.go` | `builtinAliases` — ~75 hardcoded external→canonical overrides |
| `tools/kapow/internal/namematch/matcher.go` | `Matcher` struct with `Load`, `Match`, `MatchByOPE6/8` |
| `tools/kapow/internal/namematch/normalize_test.go` | Unit tests for `Normalize` |
| `tools/kapow/internal/namematch/matcher_test.go` | Unit + integration tests for `Matcher` |

## normalize.go

```go
func Normalize(s string) string
```

Steps in order:
1. Lowercase
2. Strip "the " prefix (after lowercasing)
3. Strip diacritics: `transform.Chain(norm.NFD, runes.Remove(runes.In(unicode.Mn)), norm.NFC)` from `golang.org/x/text`
4. Replace punctuation/special chars (keep letters, digits, spaces) with space
5. Collapse multiple spaces → single space; trim

## aliases.go

```go
// builtinAliases maps normalize(external_name) → normalize(canonical_nces_name)
// Used when external sources use names that differ from NCES canonical names.
var builtinAliases = map[string]string{
    "arizona state university":             "arizona state university-tempe",
    "penn state":                            "pennsylvania state university-main campus",
    "penn state university":                "pennsylvania state university-main campus",
    // ... ~75 total entries for well-known universities
}
```

Key examples to include (normalized form):
- Common abbreviations: "uc berkeley" → Berkeley, "ucla", "usc", "mit", "caltech"
- Multi-campus with default assumption: "arizona state", "penn state", "ohio state", etc.
- Hyphen/dash variants: "texas a&m" / "texas a and m"
- Common shortenings: "university of north carolina" → Chapel Hill campus

## matcher.go

```go
type Matcher struct {
    exact      map[string]string // strings.ToLower(name) → unit_id
    normalized map[string]string // Normalize(name) → unit_id
    alias      map[string]string // strings.ToLower(db_alias) → unit_id
    ope6       map[string]string // ope6_id → unit_id
    ope8       map[string]string // ope8_id → unit_id
}

func Load(ctx context.Context, pool *pgxpool.Pool) (*Matcher, error)
func (m *Matcher) Match(name string) (unitID string, ok bool)
func (m *Matcher) MatchByOPE6(ope6 string) (unitID string, ok bool)
func (m *Matcher) MatchByOPE8(ope8 string) (unitID string, ok bool)
```

### Load implementation

Two queries (sequential, not batch — ordering matters):

1. Load institutions:
```sql
SELECT unit_id, institution_name, ope6_id, ope8_id
FROM search_base.institution
```
Populate `exact`, `normalized`, `ope6`, `ope8` maps.

2. Load DB aliases:
```sql
SELECT unit_id, alias FROM search_base.institution_alias
```
Populate `alias` map.

Error wrapping: `fmt.Errorf("namematch: load institutions: %w", err)`

### Match implementation (3-tier + OPE fallback in Match)

```
1. key = strings.ToLower(name)
2. if uid, ok := m.exact[key]; ok → return uid, true
3. nkey = Normalize(name)
4. if uid, ok := m.normalized[nkey]; ok → return uid, true
5. if canonical, ok := builtinAliases[nkey]; ok {
       if uid, ok := m.normalized[canonical]; ok → return uid, true
   }
6. if uid, ok := m.alias[key]; ok → return uid, true
7. return "", false
```

## Tests

### normalize_test.go (unit, no DB)

Table-driven `TestNormalize` covering:
- Diacritics stripping: "École" → "ecole"
- "The " prefix removal: "The Ohio State University" → "ohio state university"
- Punctuation removal: "St. John's University" → "st johns university"
- Whitespace collapse
- Already-normalized strings unchanged

### matcher_test.go

**Unit tests** (`TestMatcherMatch`, `TestMatcherOPE`):
- Construct `Matcher` directly (no DB) with hand-crafted maps
- Verify: exact match, normalized match, builtin alias resolution, DB alias resolution, miss case

**Integration test** (`TestIntegrationMatcherLoad`):
```go
func TestIntegrationMatcherLoad(t *testing.T) {
    if testing.Short() {
        t.Skip("skipping integration test")
    }
    dbURL := os.Getenv("DATABASE_URL")
    if dbURL == "" {
        t.Skip("DATABASE_URL not set")
    }
    // Load from real DB
    // Verify a handful of known canonical names resolve
    // Verify MatchByOPE6/OPE8 on known IDs
}
```

## Quality Gate

```bash
cd tools/kapow && go test -short ./internal/namematch/... -v -race -count=1
cd tools/kapow && go vet ./...
```

## Conventions

- Error wrapping: `fmt.Errorf("namematch: context: %w", err)`
- Logging: none in this package (pure library; caller logs)
- All map lookups use lowercase/normalized keys consistently
- No exported `init()` or globals other than `builtinAliases`
- Constructor: `Load(ctx, pool) (*Matcher, error)` — never panics
