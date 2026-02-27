// Example 3: Error handling patterns
// Tests shatter's ability to discover error paths in Go's (value, error) pattern.
//
// EXPECTED BRANCHES for SafeDivide:
//   1. denominator == 0       -> returns (0, error("division by zero"))
//   2. numerator == 0         -> returns (0, nil)
//   3. normal division        -> returns (result, nil)
//
// EXPECTED BRANCHES for ClassifyAge:
//   1. age < 0                -> returns ("", error("negative age"))
//   2. age == 0               -> returns ("newborn", nil)
//   3. age < 13               -> returns ("child", nil)
//   4. age < 18               -> returns ("teenager", nil)
//   5. age < 65               -> returns ("adult", nil)
//   6. age >= 65              -> returns ("senior", nil)

package examples

import "fmt"

// SafeDivide performs integer division with error handling.
func SafeDivide(numerator, denominator int) (int, error) {
	if denominator == 0 {
		return 0, fmt.Errorf("division by zero")
	}
	if numerator == 0 {
		return 0, nil
	}
	return numerator / denominator, nil
}

// ClassifyAge maps an age to a life-stage label, returning an error for invalid input.
func ClassifyAge(age int) (string, error) {
	if age < 0 {
		return "", fmt.Errorf("negative age: %d", age)
	}
	if age == 0 {
		return "newborn", nil
	}
	if age < 13 {
		return "child", nil
	}
	if age < 18 {
		return "teenager", nil
	}
	if age < 65 {
		return "adult", nil
	}
	return "senior", nil
}
