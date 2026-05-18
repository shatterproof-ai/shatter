package testdata

// ByteSliceParam takes a []byte parameter. str-ieuc regression: the analyzer
// must report the slice element as the `go_byte` complex kind, not `int`, so
// the core generates values in [0, 255] that json.Unmarshal accepts.
func ByteSliceParam(buf []byte) int {
	return len(buf)
}

// Uint8SliceParam is the explicit `[]uint8` spelling of the same shape.
func Uint8SliceParam(data []uint8) int {
	return len(data)
}

// ByteParam takes a scalar byte; should also report as `go_byte`.
func ByteParam(b byte) int {
	return int(b)
}
