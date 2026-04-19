package overlaypkg

import "testing"

func TestClassifyPositive(t *testing.T) {
	if got := Classify(1); got != "[pos]" {
		t.Fatalf("Classify(1) = %q, want [pos]", got)
	}
}
