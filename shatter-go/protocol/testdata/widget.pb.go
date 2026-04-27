package testdata

// FilenameMarkedGenerated is exported but should never be analyzed because
// the filename matches a known generated-code pattern (*.pb.go). Note this
// file deliberately omits the "Code generated ... DO NOT EDIT." header to
// exercise filename-only detection.
func FilenameMarkedGenerated(a, b int) int {
	return a * b
}
