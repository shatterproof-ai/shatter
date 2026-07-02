// Package errorparam is the str-jn9r0 E2E fixture for the Go frontend's bare
// builtin `error` parameter path. The Go analyzer maps `error` to
// ComplexKind "error" and the Rust core's random generator emits the
// cross-frontend `{"__complex_type":"error","class":...,"message":m}` shape;
// the wrapper helper writeErrorParamDeserialization decodes that object to
// errors.New(message) and decodes JSON null to a nil error.
//
// Two reachable branches cover the nil and non-nil cases.
package errorparam

// Classify returns a code based on whether err is nil:
//
//	err == nil -> "ok"
//	err != nil -> "err"
func Classify(err error) string {
	if err == nil {
		return "ok"
	}
	return "err"
}
