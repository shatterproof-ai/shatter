// Package counter is the str-jeen.50 regression fixture for pointer-receiver
// methods on a struct WITHOUT a same-package constructor. The receiver
// planner's `fallback_zero_value` strategy (str-qo1.9) emits a `zero_value`
// plan that dispatches through the launcher wrapper's `zero_value` switch
// case as `&Counter{}` — exercising the wrapper without any "unknown
// receiver kind" failure when a plan is attached.
package counter

// Counter is a pointer-receiver target type with no exported constructor.
// The planner has no same-package, nearby-package, composite-literal, or
// useful-zero-value strategy available, so PlanReceivers falls back to
// `ReceiverPlanKindFallbackZeroValue`.
type Counter struct{}

// Classify branches on the input so the concolic engine has two reachable
// paths to discover. Body intentionally does not touch receiver state — a
// zero-value `&Counter{}` is sufficient to dispatch.
func (c *Counter) Classify(n int) string {
	if n > 0 {
		return "positive"
	}
	return "non-positive"
}
