# Plan: Implement tools/kapow/internal/csvutil

## Context
The kapow data importer tool needs a reusable CSV/TSV reader that maps columns by
header name rather than positional index. This avoids brittle position-dependent
parsing when data sources add/remove/reorder columns. The package is the first
step in building importers for government data sources (NCES, etc.) that ship
as CSV or TSV files.

## Files to create

### `tools/kapow/internal/csvutil/csvutil.go`
Core implementation:

```
package csvutil

import (
    "encoding/csv"
    "fmt"
    "io"
    "strconv"
)

// Option configures a Reader.
type Option func(*Reader)

func WithDelimiter(r rune) Option { ... }
func WithLazyQuotes(b bool) Option { ... }

// Reader wraps encoding/csv and maps columns by header name.
type Reader struct {
    csv     *csv.Reader
    headers map[string]int  // col name → 0-based index
}

func NewReader(r io.Reader, opts ...Option) (*Reader, error)

// Next advances to the next record. Returns (false, nil) at EOF.
func (r *Reader) Next() (Row, bool, error)

// Row holds one parsed record with header-mapped access.
type Row struct {
    values  []string
    headers map[string]int
}

// Accessors — missing column or parse error returns zero/nil
func (r Row) String(col string) string
func (r Row) Int(col string) int
func (r Row) Float(col string) float64
func (r Row) OptionalString(col string) *string    // nil if col missing or empty
func (r Row) OptionalInt(col string) *int          // nil if col missing or empty
func (r Row) OptionalFloat(col string) *float64    // nil if col missing or empty
```

Error wrapping convention: `fmt.Errorf("csvutil: context: %w", err)`

### `tools/kapow/internal/csvutil/csvutil_test.go`
Embedded test data using `//go:embed` or inline string constants.

Test cases:
1. **Normal CSV parsing** — standard comma delimiter, multi-column, multi-row
2. **TSV parsing** — `WithDelimiter('\t')`
3. **Missing columns** — accessing a column not in the header returns zero/nil
4. **Type coercion** — Int, Float parse correctly from string values
5. **Quoted values** — values with embedded commas/newlines in quotes
6. **LazyQuotes** — `WithLazyQuotes(true)` handles malformed quotes gracefully
7. **Empty optional fields** — `OptionalString`/`OptionalInt`/`OptionalFloat` return nil for empty cells
8. **EOF handling** — Next returns (Row{}, false, nil) at end of file
9. **Header-only file** — no data rows; Next immediately returns false

## Design decisions
- `NewReader` reads the first line as headers immediately; returns error if CSV is malformed at header read
- Missing column access is silent (returns zero value) — callers don't need to guard every field access
- `OptionalString` returns nil for both missing-column AND empty-string cases, matching typical "nullable field" semantics for database imports
- `Reader` does not buffer all rows; it streams row by row via `Next()`

## Verification
```bash
cd tools/kapow && go test ./internal/csvutil/... -v -race -count=1
cd tools/kapow && go vet ./...
```
Both must pass with zero errors.
