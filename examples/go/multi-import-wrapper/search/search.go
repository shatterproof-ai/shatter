// Package search is a local stub mimicking an application search package.
// See examples/go/multi-import-wrapper/pgx for the rationale.
package search

// Query is a value-typed search request used to exercise another local
// non-stdlib package import in the multi-import wrapper fixture.
type Query struct {
	Term  string
	Limit int
}
