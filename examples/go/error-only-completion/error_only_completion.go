// Package erroronlycompletion is a regression fixture for str-jeen.53.
//
// Shatter scan reports must distinguish completed functions whose
// discovered inputs include at least one non-throwing execution
// (`completion_outcome: behavioral`) from completed functions whose
// discovered inputs all panic (`completion_outcome: error_only`). This
// fixture exercises both outcomes side-by-side so the scan report's
// per-function `completion_outcome` field and the codebase-level
// `completed_with_behavior` / `completed_error_only` counts can be
// asserted on a real Go target.
//
// `DoubleNonNegative` returns successfully for any non-negative input
// and panics for negative input — the scan therefore discovers at
// least one non-throwing execution and the function classifies as
// `behavioral`.
//
// `AlwaysPanic` panics on every input — every discovered execution
// throws, so the function classifies as `error_only`. A scan that
// reports `AlwaysPanic` as cleanly completed is the str-jeen.53
// failure mode.
package erroronlycompletion

import "fmt"

// DoubleNonNegative returns 2*n for n >= 0 and panics for n < 0. The
// non-negative branch gives the scan a non-throwing discovered input;
// the negative branch gives it a throwing one. Expected outcome:
// `completion_outcome: behavioral`.
func DoubleNonNegative(n int) int {
	if n < 0 {
		panic(fmt.Sprintf("negative input not allowed: %d", n))
	}
	return n * 2
}

// AlwaysPanic panics on every input. Every discovered execution
// throws, so the scan must classify this as
// `completion_outcome: error_only` rather than mixing it in with
// behaviorally-explored completions.
func AlwaysPanic(n int) int {
	panic(fmt.Sprintf("intentional panic: %d", n))
}
