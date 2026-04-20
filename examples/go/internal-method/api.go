// Package spike wires the internal service into a simple public API.
// This file imports the internal/svc package to verify that the Shatter Go
// frontend can analyze packages that use Go's internal import mechanism
// without emitting visibility errors (str-hy9b.C2).
package spike

import "example.com/spike/internal/svc"

// Process wraps svc.Service.DoIt in a public function that applies a sign
// transformation: positive inputs return 1, non-positive return -1.
func Process(x int) int {
	s := svc.New()
	return s.DoIt(x)
}
