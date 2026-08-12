package instrument

import (
	"go/ast"
	"go/parser"
	"go/token"
	"testing"

	"pgregory.net/rapid"
)

// duplicateFuncDecls parses generated mock support source and returns the names
// of any function declared more than once. Parsing (rather than string
// matching) is what the Go compiler would do to the harness file, so a
// duplicate found here is exactly the "redeclared in this block" build failure.
func duplicateFuncDecls(t *testing.T, source string) []string {
	t.Helper()
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "shatter_mocks.go", source, 0)
	if err != nil {
		t.Fatalf("generated mock source does not parse: %v\n%s", err, source)
	}
	seen := make(map[string]int)
	var dupes []string
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok || fn.Recv != nil {
			continue
		}
		name := fn.Name.Name
		seen[name]++
		if seen[name] == 2 {
			dupes = append(dupes, name)
		}
	}
	return dupes
}

// TestGenerateLoopMockFileDisambiguatesCollidingNames covers str-heegk: since
// str-djcv2 keyed DedupeMocks by package identity, two wire mocks spelled
// "a.util.Do" and "a/util.Do" name distinct package identities and both survive
// dedupe, yet sanitizeMockName flattens both to "a_util_Do". Emitting the shim
// for each would declare func ShatterMock_a_util_Do twice and the harness would
// not compile.
func TestGenerateLoopMockFileDisambiguatesCollidingNames(t *testing.T) {
	mocks := []MockConfig{
		{Symbol: "a.util.Do", ReturnValues: []any{1}},
		{Symbol: "a/util.Do", ReturnValues: []any{2}},
	}
	deduped := DedupeMocks(mocks, nil)
	source := generateLoopMockFile(deduped)

	if dupes := duplicateFuncDecls(t, source); len(dupes) > 0 {
		t.Fatalf("generated mock source declares %v more than once:\n%s", dupes, source)
	}

	// Whichever way the collision is resolved, the first-seen spelling keeps the
	// plain sanitized name so existing harness references stay stable.
	if !containsFunc(t, source, "ShatterMock_"+sanitizeMockName(mocks[0].Symbol)) {
		t.Fatalf("first mock lost its canonical shim name:\n%s", source)
	}
}

// TestGenerateLoopMockFileCollisionAcrossBehaviors guards the throw_error path,
// which emits two shims (ShatterMock_ and ShatterMockErr_) per mock and so has
// twice the collision surface.
func TestGenerateLoopMockFileCollisionAcrossBehaviors(t *testing.T) {
	mocks := []MockConfig{
		{Symbol: "a.util.Do", ReturnValues: []any{"boom"}, DefaultBehavior: BehaviorThrowError},
		{Symbol: "a/util.Do", ReturnValues: []any{3}},
		{Symbol: "b/util.Do", ReturnValues: []any{"bang"}, DefaultBehavior: BehaviorThrowError},
	}
	source := generateLoopMockFile(DedupeMocks(mocks, nil))
	if dupes := duplicateFuncDecls(t, source); len(dupes) > 0 {
		t.Fatalf("generated mock source declares %v more than once:\n%s", dupes, source)
	}
}

// TestPropertyGeneratedMockFuncNamesUnique asserts the general invariant: no
// arrangement of mock symbols may make generateLoopMockFile emit the same
// function name twice.
func TestPropertyGeneratedMockFuncNamesUnique(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		n := rapid.IntRange(1, 6).Draw(t, "nMocks")
		mocks := make([]MockConfig, n)
		for i := range mocks {
			mocks[i] = MockConfig{
				// Symbols drawn from a tiny alphabet of separators so distinct
				// spellings routinely sanitize to the same identifier.
				Symbol:       rapid.StringMatching(`[ab][./:_][ab][./:_][ab]`).Draw(t, "symbol"),
				ReturnValues: []any{i},
				DefaultBehavior: rapid.SampledFrom([]string{
					BehaviorRepeatLast, BehaviorCycle, BehaviorThrowError, BehaviorPassthrough,
				}).Draw(t, "behavior"),
			}
		}
		source := generateLoopMockFile(mocks)
		fset := token.NewFileSet()
		file, err := parser.ParseFile(fset, "shatter_mocks.go", source, 0)
		if err != nil {
			t.Fatalf("generated mock source does not parse: %v\n%s", err, source)
		}
		seen := make(map[string]bool)
		for _, decl := range file.Decls {
			fn, ok := decl.(*ast.FuncDecl)
			if !ok || fn.Recv != nil {
				continue
			}
			if seen[fn.Name.Name] {
				t.Fatalf("duplicate func %s for mocks %+v:\n%s", fn.Name.Name, mocks, source)
			}
			seen[fn.Name.Name] = true
		}
	})
}

// containsFunc reports whether source declares a top-level function with name.
func containsFunc(t *testing.T, source, name string) bool {
	t.Helper()
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "shatter_mocks.go", source, 0)
	if err != nil {
		t.Fatalf("generated mock source does not parse: %v", err)
	}
	for _, decl := range file.Decls {
		if fn, ok := decl.(*ast.FuncDecl); ok && fn.Recv == nil && fn.Name.Name == name {
			return true
		}
	}
	return false
}
