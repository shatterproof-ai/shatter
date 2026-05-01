// Package config is a local stub mimicking an application config package.
// See examples/go/multi-import-wrapper/pgx for the rationale.
package config

// Config is a value-typed configuration record used to exercise yet another
// local non-stdlib package import in the multi-import wrapper fixture.
type Config struct {
	Host string
	Port int
}
