package testdata

// wrapper is a struct with no exported fields.
type wrapper struct {
	inner string
	count int
}

// ProcessWrapper takes a wrapper param — the struct has no exported fields,
// so TypeInfo should have kind "object" with an empty fields array.
func ProcessWrapper(w wrapper) string {
	return w.inner
}

// EmptyStruct has zero fields at all.
type EmptyStruct struct{}

// ProcessEmpty takes an empty struct.
func ProcessEmpty(e EmptyStruct) int {
	return 42
}
