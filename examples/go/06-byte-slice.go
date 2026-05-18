// Package main is the str-ieuc fixture: a `[]byte` parameter that branches
// on byte-range boundaries (0, 1, 255). The Go analyzer must report the
// slice element as the `go_byte` complex kind so the core generates valid
// uint8 elements that `json.Unmarshal` accepts before the target ever runs.
package main

// ClassifyBytes returns a different code depending on the contents of buf:
//
//   - 0: empty slice
//   - 1: first byte is 0
//   - 2: first byte is 1
//   - 3: first byte is 255
//   - 4: any other content
//
// All boundary inputs are reachable only if generation stays in [0, 255].
func ClassifyBytes(buf []byte) int {
	if len(buf) == 0 {
		return 0
	}
	switch buf[0] {
	case 0:
		return 1
	case 1:
		return 2
	case 255:
		return 3
	}
	return 4
}
