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
		"__shatter_record_scope",
		"__shatter_dump_results",
		"__shatter_get_results",
	} {
		if !strings.Contains(src, "func "+fn) {
			t.Errorf("recorder missing function %s", fn)
		}
	}
}

// TestGeneratedRecorderUsesNamespacedBoolPtr is the str-jeen.74 regression:
// the recorder emits a bool-pointer helper at package scope. If the helper is
// named "boolPtr", it collides with any same-named function the target package
// already defines, producing "boolPtr redeclared in this block". The helper
// must use the __shatter prefix so it is collision-safe.
func TestGeneratedRecorderUsesNamespacedBoolPtr(t *testing.T) {
	src := generateRecorder("main")
	if strings.Contains(src, "func boolPtr(") {
		t.Errorf("recorder emits 'func boolPtr(' — must use '__shatterBoolPtr' to avoid collisions with user code\nsource snippet:\n%s", src)
	}
	if !strings.Contains(src, "func __shatterBoolPtr(") {
		t.Errorf("recorder missing 'func __shatterBoolPtr(' — helper must be namespaced\nsource snippet:\n%s", src)
	}
	// All call sites must also use the namespaced form.
	if strings.Contains(src, "boolPtr(") && !strings.Contains(src, "__shatterBoolPtr(") {
		t.Errorf("recorder call site still uses un-namespaced boolPtr()")
	}
}

func TestGeneratedRecorderContainsTraceTypes(t *testing.T) {
	src := generateRecorder("main")
	for _, typeName := range []string{
		"__shatterScopeEvent",
		"__shatterTraceEvent",
	} {
		if !strings.Contains(src, "type "+typeName+" struct") {
			t.Errorf("recorder missing type %s", typeName)
		}
	}
}
