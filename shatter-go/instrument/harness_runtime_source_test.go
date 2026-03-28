package instrument

import (
	"go/parser"
	"go/token"
	"strings"
	"testing"
)

func TestHarnessRuntimeSourceParses(t *testing.T) {
	src := harnessRuntimeSource()
	fset := token.NewFileSet()
	_, err := parser.ParseFile(fset, "runtime.go", src, 0)
	if err != nil {
		t.Errorf("harnessRuntimeSource does not parse: %v", err)
	}
}

func TestHarnessRuntimeSourceContainsTypes(t *testing.T) {
	src := harnessRuntimeSource()

	requiredTypes := []string{
		"type Request struct",
		"type SideEffect struct",
		"type Error struct",
		"type Perf struct",
		"type Response struct",
		"type Capture struct",
		"type PerfSnap struct",
	}
	for _, typ := range requiredTypes {
		if !strings.Contains(src, typ) {
			t.Errorf("missing type definition: %s", typ)
		}
	}
}

func TestHarnessRuntimeSourceContainsFunctions(t *testing.T) {
	src := harnessRuntimeSource()

	requiredFuncs := []string{
		"func RunLoop(",
		"func CaptureConsole()",
		"func (c *Capture) Stop()",
		"func StartPerf()",
		"func (s *PerfSnap) Finish()",
		"func SafeCall(",
		"func ConsoleSideEffects(",
	}
	for _, fn := range requiredFuncs {
		if !strings.Contains(src, fn) {
			t.Errorf("missing function: %s", fn)
		}
	}
}

func TestHarnessRuntimeSourceEOFExit(t *testing.T) {
	src := harnessRuntimeSource()
	if !strings.Contains(src, "os.Exit(1)") {
		t.Error("harness runtime should exit with code 1 on EOF")
	}
}

func TestHarnessRuntimeSourcePackageName(t *testing.T) {
	src := harnessRuntimeSource()
	if !strings.HasPrefix(src, "package harness") {
		t.Error("harness runtime source should use package harness")
	}
}

func TestHarnessRuntimeSourceExternalCallsField(t *testing.T) {
	src := harnessRuntimeSource()
	// The shared runtime always includes ExternalCalls as json.RawMessage with omitempty.
	// When nil, it's omitted from JSON output. No hasMocks parameter needed.
	if !strings.Contains(src, "ExternalCalls") {
		t.Error("response should include ExternalCalls field")
	}
	if !strings.Contains(src, "omitempty") {
		t.Error("ExternalCalls should have omitempty tag")
	}
}
