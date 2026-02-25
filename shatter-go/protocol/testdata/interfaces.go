package testdata

import "fmt"

// Stringer is a simple interface.
type Stringer interface {
	String() string
}

// FormatValue formats a value using its Stringer implementation.
func FormatValue(s Stringer) string {
	return fmt.Sprintf("value: %s", s.String())
}

// FormatAny formats any value.
func FormatAny(v interface{}) string {
	return fmt.Sprintf("%v", v)
}
