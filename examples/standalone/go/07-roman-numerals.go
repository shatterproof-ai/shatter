// Example 7: Roman numeral converter
// Converts integers to Roman numeral strings and back. Exercises cascading
// range checks and character-by-character parsing.
//
// EXPECTED BRANCHES for IntToRoman (16):
//   1. n <= 0                                   → error: "out of range"
//   2. n > 3999                                 → error: "out of range"
//   3. n >= 1000                                → appends "M", subtracts 1000
//   4. n >= 900                                 → appends "CM", subtracts 900
//   5. n >= 500                                 → appends "D", subtracts 500
//   6. n >= 400                                 → appends "CD", subtracts 400
//   7. n >= 100                                 → appends "C", subtracts 100
//   8. n >= 90                                  → appends "XC", subtracts 90
//   9. n >= 50                                  → appends "L", subtracts 50
//  10. n >= 40                                  → appends "XL", subtracts 40
//  11. n >= 10                                  → appends "X", subtracts 10
//  12. n >= 9                                   → appends "IX", subtracts 9
//  13. n >= 5                                   → appends "V", subtracts 5
//  14. n >= 4                                   → appends "IV", subtracts 4
//  15. n >= 1                                   → appends "I", subtracts 1
//  16. n == 0 (exhausted)                       → returns accumulated string
//
// EXPECTED BRANCHES for RomanToInt (18):
//   1. empty string                             → 0
//   2. invalid character                        → error: "invalid roman numeral"
//   3. char 'M'                                 → adds 1000
//   4. char 'D'                                 → adds 500
//   5. char 'C' followed by 'M'                → adds 900 (subtractive)
//   6. char 'C' followed by 'D'                → adds 400 (subtractive)
//   7. char 'C' (normal)                        → adds 100
//   8. char 'L'                                 → adds 50
//   9. char 'X' followed by 'C'                → adds 90 (subtractive)
//  10. char 'X' followed by 'L'                → adds 40 (subtractive)
//  11. char 'X' (normal)                        → adds 10
//  12. char 'V'                                 → adds 5
//  13. char 'I' followed by 'X'                → adds 9 (subtractive)
//  14. char 'I' followed by 'V'                → adds 4 (subtractive)
//  15. char 'I' (normal)                        → adds 1
//  16. subtractive pair consumed                → skip next char
//  17. end of string                            → return total
//  18. lowercase input                          → uppercased before processing

package main

import (
	"errors"
	"strings"
)

type romanPair struct {
	numeral string
	value   int
}

var romanValues = []romanPair{
	{"M", 1000}, {"CM", 900}, {"D", 500}, {"CD", 400},
	{"C", 100}, {"XC", 90}, {"L", 50}, {"XL", 40},
	{"X", 10}, {"IX", 9}, {"V", 5}, {"IV", 4}, {"I", 1},
}

var charValues = map[byte]int{
	'M': 1000, 'D': 500, 'C': 100, 'L': 50, 'X': 10, 'V': 5, 'I': 1,
}

// IntToRoman converts an integer (1-3999) to its Roman numeral representation.
// Returns an error for values outside the valid range.
func IntToRoman(n int) (string, error) {
	if n <= 0 || n > 3999 {
		return "", errors.New("out of range")
	}

	var result strings.Builder
	remaining := n

	for _, pair := range romanValues {
		for remaining >= pair.value {
			result.WriteString(pair.numeral)
			remaining -= pair.value
		}
	}

	return result.String(), nil
}

// RomanToInt converts a Roman numeral string to its integer value.
// Returns 0 for empty strings. Returns an error for invalid characters.
func RomanToInt(s string) (int, error) {
	if len(s) == 0 {
		return 0, nil
	}

	upper := strings.ToUpper(s)
	total := 0
	i := 0

	for i < len(upper) {
		val, ok := charValues[upper[i]]
		if !ok {
			return 0, errors.New("invalid roman numeral")
		}

		if i+1 < len(upper) {
			next, nextOk := charValues[upper[i+1]]
			if nextOk && next > val {
				total += next - val
				i += 2
				continue
			}
		}

		total += val
		i++
	}

	return total, nil
}
