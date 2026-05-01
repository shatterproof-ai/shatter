// Internal sibling test: declared `package main`, the same package as the
// target source file. Its mere presence in the directory is what triggered
// the original str-x0sv mixed-package failure once the overlay renamed
// admissions.go to `shattertarget` — the loader saw both `shattertarget`
// (target) and `main` (this file) in the same directory.

package main

import "testing"

func TestComputePositive(t *testing.T) {
	if got := Compute(1); got != "positive" {
		t.Fatalf("Compute(1) = %q, want \"positive\"", got)
	}
}
