package testdata

// FileWriter and BufferWriter both expose a method named Write on the same
// file. Before str-fuhw.1.1, scan internals keyed function lookups on the
// bare AST name (`fn.Name.Name`) and a qualified ID of the form
// "<file>::<name>", so the two methods collapsed to one key and only one
// reached the test order, the analysis map, and the report.
//
// The fixture is intentionally minimal: each method takes a single byte
// slice and returns nothing, so the analysis carries the same shape for
// both. The only thing distinguishing them is the receiver type.

type FileWriter struct{}

// Write writes p as if to a file. Same bare name as BufferWriter.Write.
func (f *FileWriter) Write(p []byte) {
	_ = p
}

type BufferWriter struct{}

// Write writes p as if to an in-memory buffer. Same bare name as
// FileWriter.Write but a different receiver type.
func (b *BufferWriter) Write(p []byte) {
	_ = p
}
