// Package dep is the "real dependency" of the str-c8djq execute-time mock
// substitution E2E fixture. Its exported Fetch has an observable "real ran"
// behavior: it always returns a Result whose Code is a fixed sentinel (999)
// regardless of its input.
//
// The sentinel is intentionally chosen to be far outside the range the
// configured mock expression can ever produce, so the target's sentinel branch
// is reachable ONLY when the real Fetch runs. The E2E asserts that branch never
// appears, proving the call site was rewritten to the mock expression and the
// real body never executed. No panic / no network — the sentinel is a plain
// return value the harness can observe end-to-end.
package dep

// SentinelCode is the Code the real Fetch always returns. Any branch keyed on
// this value is unreachable once the call site is mock-substituted.
const SentinelCode = 999

// Result is the value produced by the real dependency. Returning a dep-package
// type (rather than a bare int) means a faithful mock expression must also
// reference the dep package, which keeps the target file's `dep` import live
// after call-site substitution — mirroring the working builder-level fixture
// shape (&dep.Thing{...}). Substitution replaces the whole call, not the
// imports, so an int-returning mock that dropped the only dep reference would
// fail to compile with "imported and not used".
type Result struct {
	Code int
}

// Fetch is the real dependency call. It ignores x and returns the sentinel
// Result, so under the real implementation Classify always lands in its
// sentinel branch. The configured mock replaces the whole `dep.Fetch(x)` call
// site with a bounded expression that can never equal the sentinel.
func Fetch(x int) Result {
	return Result{Code: SentinelCode}
}
