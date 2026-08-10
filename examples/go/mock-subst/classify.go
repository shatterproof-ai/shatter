// Package mocksubst is the str-c8djq E2E fixture for execute-time Go mock
// substitution through the full pipeline (Rust core -> real shatter-go
// subprocess -> overlay build with a config-driven call-site rewrite).
//
// Classify calls dep.Fetch(x) and branches on the resulting Code. The real
// dep.Fetch ignores x and always returns Code 999 (dep.SentinelCode), so under
// the real dependency Classify can only ever return "sentinel". The fixture's
// `.shatter/config.yaml` mocks "dep.Fetch" with the bounded Go expression
// `dep.Result{Code: x % 7}`, whose Code lies in [-6, 6] and can never reach the
// >= 100 sentinel gate. So:
//
//   - Under the REAL dependency: only "sentinel" is reachable.
//   - Under the MOCK expression: only "mock-neg" / "mock-zero" / "mock-pos"
//     are reachable, and "sentinel" is unreachable by construction.
//
// The mock expression references the dep package (dep.Result{...}), which keeps
// the target file's `dep` import live after the call site is rewritten — the
// same shape the builder-level fixture uses (&dep.Thing{...}).
//
// The E2E asserts the mock-only outcomes appear and "sentinel" never does,
// proving the real dep.Fetch body never ran.
package mocksubst

import "example.com/mock-subst/dep"

// Classify buckets the Code produced by dep.Fetch(x). The sentinel gate
// (>= 100) is only satisfied by the real dependency's 999; the mock expression
// `dep.Result{Code: x % 7}` stays within [-6, 6] and drives the three
// mock-only buckets.
func Classify(x int) string {
	r := dep.Fetch(x)
	if r.Code >= 100 {
		return "sentinel"
	}
	if r.Code < 0 {
		return "mock-neg"
	}
	if r.Code == 0 {
		return "mock-zero"
	}
	return "mock-pos"
}
