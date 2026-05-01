// External sibling test: declared `package main_test`. Go's test loader
// treats `_test` packages as a parallel compilation unit, but
// `go build -overlay` (used by the shatter-go builder) still scans the
// directory for declared packages and would surface this file as
// `main_test` if the overlay missed it. The str-x0sv fix stages an
// overlay-rewritten copy declared as `shattertarget_test`, which keeps
// the build's directory view consistent.

package main_test

import "testing"

func TestExternal(t *testing.T) {
	_ = t
}
