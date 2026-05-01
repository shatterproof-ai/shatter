// Package gqlerror is a local stub mimicking gqlgen's gqlerror package.
// See examples/go/multi-import-wrapper/pgx for the rationale.
package gqlerror

// Error stands in for *gqlerror.Error.
type Error struct {
	Message string
	Path    []string
}
