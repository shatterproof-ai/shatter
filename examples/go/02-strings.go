// Example 2: String operations with branches
// Tests shatter's ability to reason about string properties in Go.
//
// EXPECTED BRANCHES for ClassifyString:
//   1. s == ""               -> "empty"
//   2. len(s) == 1           -> "single-char"
//   3. starts with "http"    -> "url"
//   4. contains "@"          -> "email-like"
//   5. none of the above     -> "text"
//
// EXPECTED BRANCHES for TransformString:
//   1. mode == "upper"   -> uppercased input
//   2. mode == "lower"   -> lowercased input
//   3. mode == "repeat" AND count <= 0 -> ""
//   4. mode == "repeat" AND count > 0  -> input repeated count times
//   5. unknown mode      -> input unchanged

package examples

import "strings"

// ClassifyString categorizes a string by its content characteristics.
func ClassifyString(s string) string {
	if s == "" {
		return "empty"
	}
	if len(s) == 1 {
		return "single-char"
	}
	if len(s) >= 4 && s[:4] == "http" {
		return "url"
	}
	if strings.Contains(s, "@") {
		return "email-like"
	}
	return "text"
}

// TransformString applies a transformation mode to the input string.
func TransformString(input string, mode string, count int) string {
	if mode == "upper" {
		return strings.ToUpper(input)
	}
	if mode == "lower" {
		return strings.ToLower(input)
	}
	if mode == "repeat" {
		if count <= 0 {
			return ""
		}
		return strings.Repeat(input, count)
	}
	return input
}
