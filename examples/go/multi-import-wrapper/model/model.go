// Package model is a local stub mimicking an application model package.
// See examples/go/multi-import-wrapper/pgx for the rationale.
package model

// User is a value-typed model record used to exercise an unqualified-package
// value-type parameter in the multi-import wrapper fixture.
type User struct {
	ID   int64
	Name string
}
