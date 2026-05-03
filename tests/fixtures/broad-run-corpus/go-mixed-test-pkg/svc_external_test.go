package svc_test

import (
	"testing"

	"example.com/mixedtestpkg"
)

func TestCategorizeExternal(t *testing.T) {
	if got := svc.Categorize(0); got != "non-neg" {
		t.Fatalf("Categorize(0) = %q", got)
	}
}
