// Package calc is the str-jeen.50 regression fixture for value-receiver
// methods on a composite-literal-safe struct. The receiver planner emits a
// `composite_literal` plan (ReceiverKind = "zero_value"), which dispatches
// through the launcher wrapper's value-receiver zero-value path — proving
// the wrapper case compiles and runs cleanly without "unknown receiver
// kind" when a plan is attached.
package calc

// Calc is a value-receiver target type. No same-package constructor is
// declared and the receiver type has no exported fields, so the
// composite-literal strategy is the planner's choice.
type Calc struct{}

// Sign branches on the input and returns a tri-valued classification. The
// method has a value receiver (not pointer) so the wrapper's zero-value
// case emits `var _recv Calc` rather than `&Calc{}`.
func (c Calc) Sign(n int) string {
	if n > 0 {
		return "pos"
	}
	if n < 0 {
		return "neg"
	}
	return "zero"
}
