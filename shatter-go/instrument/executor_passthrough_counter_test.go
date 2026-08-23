package instrument

import (
	"go/ast"
	"go/importer"
	"go/parser"
	"go/token"
	"go/types"
	"testing"
)

// typeCheckGeneratedSource parses and type-checks generated mock support
// source, mirroring what `go build` would do to the harness file. Unlike
// duplicateFuncDecls's plain parse, this catches undefined identifiers (e.g.
// a counter variable referenced in shatterResetMockCounters but never
// declared), not just syntax errors.
func typeCheckGeneratedSource(t *testing.T, source string) error {
	t.Helper()
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "shatter_mocks.go", source, 0)
	if err != nil {
		t.Fatalf("generated mock source does not parse: %v\n%s", err, source)
	}
	conf := types.Config{Importer: importer.ForCompiler(fset, "source", nil)}
	_, err = conf.Check("main", fset, []*ast.File{file}, nil)
	return err
}

// TestGenerateLoopMockFile_PassthroughOnly_Compiles covers str-zn94g: a single
// passthrough wire mock made generateLoopMockFile skip the per-mock
// declaration block (including `var shatterMock0_callIdx int64`), but
// shatterResetMockCounters unconditionally referenced shatterMock0_callIdx
// for every mock index regardless of behavior — an undefined-identifier
// compile failure in the generated harness.
func TestGenerateLoopMockFile_PassthroughOnly_Compiles(t *testing.T) {
	mocks := []MockConfig{
		{Symbol: "a.util.Do", ReturnValues: []any{1}, DefaultBehavior: BehaviorPassthrough},
	}
	source := generateLoopMockFile(mocks)
	if err := typeCheckGeneratedSource(t, source); err != nil {
		t.Fatalf("generated mock source does not compile: %v\n%s", err, source)
	}
}

// TestGenerateLoopMockFile_MixedPassthroughAndNonPassthrough_Compiles covers
// the mixed case: a passthrough mock alongside a non-passthrough one, so the
// counter-index alignment between the emission loop and the reset loop must
// stay correct for every index, not just when all mocks share one behavior.
func TestGenerateLoopMockFile_MixedPassthroughAndNonPassthrough_Compiles(t *testing.T) {
	mocks := []MockConfig{
		{Symbol: "a.util.Do", ReturnValues: []any{1}, DefaultBehavior: BehaviorPassthrough},
		{Symbol: "b.util.Do", ReturnValues: []any{2}, DefaultBehavior: BehaviorRepeatLast},
	}
	source := generateLoopMockFile(mocks)
	if err := typeCheckGeneratedSource(t, source); err != nil {
		t.Fatalf("generated mock source does not compile: %v\n%s", err, source)
	}
}
