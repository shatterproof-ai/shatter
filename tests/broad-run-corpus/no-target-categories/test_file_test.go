// NoTargetReason::TestFile — a Go `_test.go` file. The Go frontend skips
// these by name convention.
package broadrun

import "testing"

func TestPlaceholder(t *testing.T) {
	if 1+1 != 2 {
		t.Fatal("math is broken")
	}
}
