// Package variadicsum is the str-3op0 E2E gate fixture for the Go frontend's
// variadic-helper shape. The function combines a variadic int parameter with
// a single non-variadic int parameter and returns one of two string labels
// based on whether the sum of the variadic slice meets the threshold.
//
// Two reachable branches:
//
//	sum(vals...) >= threshold  ->  "above"
//	sum(vals...) <  threshold  ->  "below"
//
// The variadic parameter exercises the launcher's variadic-wrapper path
// (str-jeen.48 was the regression that motivated this gate) end-to-end
// through analyze -> instrument -> orchestrator-driven explore -> Z3 solve.
package variadicsum

// SumThreshold returns "above" if the sum of vals is at least threshold,
// otherwise "below". Both branches are reachable; Z3 should drive the
// explorer to a vals slice whose sum crosses the threshold.
func SumThreshold(threshold int, vals ...int) string {
	total := 0
	for _, v := range vals {
		total += v
	}
	if total >= threshold {
		return "above"
	}
	return "below"
}
