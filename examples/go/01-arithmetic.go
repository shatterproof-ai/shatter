// Example 1: Pure arithmetic with branches
// Tests shatter's ability to explore numeric conditions in Go.
//
// EXPECTED BRANCHES for ClassifyNumber:
//   1. n < 0        -> returns "negative"
//   2. n == 0       -> returns "zero"
//   3. n > 0, even  -> returns "positive-even"
//   4. n > 0, odd   -> returns "positive-odd"
//
// EXPECTED BRANCHES for CompareMagnitudes:
//   1. sum > 100 AND product > 1000  -> "both-large"
//   2. sum > 100 AND product <= 1000 -> "sum-large"
//   3. sum <= 100 AND product > 1000 -> "product-large"
//   4. sum <= 100 AND product <= 1000 -> "both-small"

package examples

// ClassifyNumber categorizes an integer by sign and parity.
func ClassifyNumber(n int) string {
	if n < 0 {
		return "negative"
	}
	if n == 0 {
		return "zero"
	}
	if n%2 == 0 {
		return "positive-even"
	}
	return "positive-odd"
}

// CompareMagnitudes classifies two numbers by comparing their sum and product
// against thresholds.
func CompareMagnitudes(a, b int) string {
	sum := a + b
	product := a * b

	if sum > 100 {
		if product > 1000 {
			return "both-large"
		}
		return "sum-large"
	}
	if product > 1000 {
		return "product-large"
	}
	return "both-small"
}
