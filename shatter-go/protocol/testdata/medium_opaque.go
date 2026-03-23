package testdata

// MediumOpaque1 has an exported ID field (so no_constructor does not apply) and a
// Close() error method — closeable_interface heuristic.
//
// Expected: param a → opaque with medium_opacity "closeable_interface"
type MediumOpaque1 struct {
	ID   int
	data []byte
}

func (m *MediumOpaque1) Close() error { return nil }

// MediumOpaque2 has an exported Name field (so no_constructor does not apply) and a
// native handle field fd — native_handle_field heuristic.
//
// Expected: param b → opaque with medium_opacity "native_handle_field"
type MediumOpaque2 struct {
	Name string
	fd   int
}

// SafeType has neither a close method nor handle fields — not flagged.
//
// Expected: param c → object or struct type (NOT opaque)
type SafeType struct {
	Name string
	Age  int
}

// UseMediumOpaqueTypes uses the above types in a function for testing.
func UseMediumOpaqueTypes(a MediumOpaque1, b MediumOpaque2, c SafeType) int {
	_ = a
	_ = b
	return c.Age
}
