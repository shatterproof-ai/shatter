package instrument

import (
	"go/parser"
	"go/token"
	"strings"
	"testing"
)

func TestGeneratedRecorderCompiles(t *testing.T) {
	src := generateRecorder("main")
	fset := token.NewFileSet()
	_, err := parser.ParseFile(fset, "recorder.go", src, 0)
	if err != nil {
		t.Fatalf("generated recorder does not parse: %v\nsource:\n%s", err, src)
	}
}

func TestGeneratedRecorderUsesCorrectPackage(t *testing.T) {
	for _, pkg := range []string{"main", "mylib", "foo_test"} {
		src := generateRecorder(pkg)
		if !strings.HasPrefix(src, "package "+pkg+"\n") {
			t.Errorf("expected package %q, got prefix: %q", pkg, src[:40])
		}
	}
}

func TestGeneratedRecorderContainsRequiredFunctions(t *testing.T) {
	src := generateRecorder("main")
	for _, fn := range []string{
		"__shatter_record_line",
		"__shatter_record_branch",
		"__shatter_dump_results",
	} {
		if !strings.Contains(src, "func "+fn) {
			t.Errorf("recorder missing function %s", fn)
		}
	}
}
