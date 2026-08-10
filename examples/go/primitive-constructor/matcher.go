// Package primitiveconstructor is the str-ozjv E2E fixture for the Go
// frontend's parameterized-constructor path with PRIMITIVE constructor
// arguments (string, float64, time.Duration).
//
// The Go wrapper decodes each constructor argument from the JSON input
// prefix. When the planner materializes those prefix slots as aggregate
// (`{}`) or null values instead of primitive JSON zero values, the wrapper's
// json.Unmarshal fails before the method body runs (str-ozjv), e.g.:
//
//	param _shatterCtorArg1: json: cannot unmarshal object into Go value of type float64
//
// The constructor parameter names are deliberately NOT path-like ("label",
// not "path"/"file"/"dir") so they do not take the temp-file constructor-seed
// branch and instead exercise the primitive zero-value materialization path.
package primitiveconstructor

import "time"

// Matcher is a stateful type whose only constructor takes three primitive
// arguments.
type Matcher struct {
	label   string
	weight  float64
	timeout time.Duration
}

// NewMatcher is a parameterized constructor whose arguments are primitive Go
// types. Each must materialize as a primitive JSON zero value in the input
// prefix ("" / 0.0 / 0), not an aggregate object.
func NewMatcher(label string, weight float64, timeout time.Duration) *Matcher {
	return &Matcher{label: label, weight: weight, timeout: timeout}
}

// Score branches on both receiver-derived state and the method parameter so
// the constructor must succeed for any branch beyond the first to be reached.
func (m *Matcher) Score(x int) string {
	if m.weight > 1.0 {
		return "heavy"
	}
	if m.timeout > 0 {
		return "bounded"
	}
	if m.label != "" {
		return "labeled"
	}
	if x > 0 {
		return "positive"
	}
	return "base"
}
