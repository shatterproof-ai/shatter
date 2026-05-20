// Package callback is the str-4cqz fixture for Go function-typed parameters.
//
// Pre-str-4cqz Shatter attempted to fill the `fn` slot with a JSON value,
// producing "param fn: json: cannot unmarshal X into Go value of type
// func(string) error" error clusters on every iteration. With str-4cqz the
// wrapper bakes `fn = nil` deterministically at build time, so the target
// reaches the `fn == nil` branch and the run completes without structural
// errors.
package callback

// ApplyCallback exercises a function-typed parameter alongside a primitive.
// The function deliberately handles `fn == nil` so the deterministic-stub
// path returns a value rather than panicking.
func ApplyCallback(s string, fn func(string) error) int {
	if fn == nil {
		if s == "" {
			return 0
		}
		return len(s)
	}
	if err := fn(s); err != nil {
		return -1
	}
	return 1
}
