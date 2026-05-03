package svc

import "testing"

func TestCategorizeInternal(t *testing.T) {
	if got := Categorize(-1); got != "neg" {
		t.Fatalf("Categorize(-1) = %q", got)
	}
}
