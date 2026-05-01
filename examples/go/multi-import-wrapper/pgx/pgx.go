// Package pgx is a local stub mimicking the popular pgx/v5 PostgreSQL driver.
// It exists only so the str-jeen.33 multi-import wrapper fixture can exercise
// a third-party-shaped package name without taking a real network dependency.
package pgx

// Conn stands in for *pgx.Conn so wrapper-gen has a concrete pointer-to-struct
// type from a non-stdlib package to resolve into an import.
type Conn struct {
	DSN string
}
