package main

import "fmt"

// Example 3: Struct parameters with field access in conditions.
// Tests shatter's ability to generate structured inputs and reason about field values.

// UserProfile — test fixture for struct-field access analysis.
type UserProfile struct {
	Name       string
	Age        int
	IsVerified bool
	Role       string
}

// CategorizeUser — 6 branches: age<0→error, age<13→"child", age<18→"teen",
// age≥18+verified+admin→"admin", age≥18+verified→"verified-user", else→"unverified-user".
// Analyzer should detect nested field checks (age ranges, boolean, string equality).
func CategorizeUser(user UserProfile) (string, error) {
	if user.Age < 0 {
		return "", fmt.Errorf("invalid age")
	}
	if user.Age < 13 {
		return "child", nil
	}
	if user.Age < 18 {
		return "teen", nil
	}
	if user.IsVerified {
		if user.Role == "admin" {
			return "admin", nil
		}
		return "verified-user", nil
	}
	return "unverified-user", nil
}

// Rectangle — test fixture for dimension-based classification.
type Rectangle struct {
	Width  float64
	Height float64
}

// DescribeRectangle — 4 branches: non-positive dimension→error,
// width==height→"square", area>10000→"large-rectangle", else→"small-rectangle".
func DescribeRectangle(rect Rectangle) (string, error) {
	if rect.Width <= 0 || rect.Height <= 0 {
		return "", fmt.Errorf("non-positive dimension")
	}
	if rect.Width == rect.Height {
		return "square", nil
	}
	area := rect.Width * rect.Height
	if area > 10000 {
		return "large-rectangle", nil
	}
	return "small-rectangle", nil
}
