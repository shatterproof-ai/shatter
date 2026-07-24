package wrapper

import (
	"fmt"
	"go/ast"
	"go/importer"
	"go/parser"
	"go/token"
	"go/types"
	"slices"
	"testing"

	"golang.org/x/tools/go/packages"
	"pgregory.net/rapid"
)

// mineFrom type-checks src (package "targets"), locates the function named
// fnName, and mines its body for error sentinels keyed by the given error
// parameter names. pkgPath is passed as "targets" so same-package sentinels
// resolve to bare names.
func mineFrom(t *testing.T, src, fnName string, errorParams ...string) map[string][]ErrorSentinel {
	t.Helper()
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "src.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Defs: map[*ast.Ident]types.Object{},
		Uses: map[*ast.Ident]types.Object{},
	}
	conf := types.Config{Importer: importer.Default()}
	if _, err := conf.Check("targets", fset, []*ast.File{file}, info); err != nil {
		t.Fatalf("type-check: %v", err)
	}
	var fn *ast.FuncDecl
	for _, decl := range file.Decls {
		if f, ok := decl.(*ast.FuncDecl); ok && f.Name.Name == fnName {
			fn = f
			break
		}
	}
	if fn == nil {
		t.Fatalf("func %q not found", fnName)
	}
	names := make(map[string]bool, len(errorParams))
	for _, n := range errorParams {
		names[n] = true
	}
	return MineErrorSentinels(fn.Body, info, "targets", names)
}

func TestMineErrorSentinels_ImportedSentinel(t *testing.T) {
	const src = `package targets

import (
	"errors"
	"io"
)

func F(err error) string {
	if errors.Is(err, io.EOF) {
		return "eof"
	}
	return "other"
}
`
	got := mineFrom(t, src, "F", "err")
	sentinels := got["err"]
	if len(sentinels) != 1 {
		t.Fatalf("len(sentinels) = %d, want 1: %+v", len(sentinels), sentinels)
	}
	if sentinels[0].Expr != "io.EOF" {
		t.Errorf("Expr = %q, want io.EOF", sentinels[0].Expr)
	}
	if sentinels[0].ImportPath != "io" {
		t.Errorf("ImportPath = %q, want io", sentinels[0].ImportPath)
	}
}

func TestMineErrorSentinels_SamePackageSentinel(t *testing.T) {
	const src = `package targets

import "errors"

var ErrLocal = errors.New("local")

func F(err error) bool {
	return errors.Is(err, ErrLocal)
}
`
	got := mineFrom(t, src, "F", "err")
	sentinels := got["err"]
	if len(sentinels) != 1 {
		t.Fatalf("len(sentinels) = %d, want 1: %+v", len(sentinels), sentinels)
	}
	if sentinels[0].Expr != "ErrLocal" {
		t.Errorf("Expr = %q, want ErrLocal", sentinels[0].Expr)
	}
	if sentinels[0].ImportPath != "" {
		t.Errorf("ImportPath = %q, want empty (same package)", sentinels[0].ImportPath)
	}
}

func TestMineErrorSentinels_MultipleDedupOrdered(t *testing.T) {
	const src = `package targets

import (
	"errors"
	"io"
	"os"
)

func F(err error) string {
	switch {
	case errors.Is(err, io.EOF):
		return "eof"
	case errors.Is(err, os.ErrNotExist):
		return "missing"
	case errors.Is(err, io.EOF): // duplicate — must not be re-added
		return "eof2"
	}
	return "other"
}
`
	got := mineFrom(t, src, "F", "err")
	sentinels := got["err"]
	if len(sentinels) != 2 {
		t.Fatalf("len(sentinels) = %d, want 2 (deduped): %+v", len(sentinels), sentinels)
	}
	// Source order preserved: io.EOF first, os.ErrNotExist second.
	if sentinels[0].Expr != "io.EOF" || sentinels[1].Expr != "os.ErrNotExist" {
		t.Errorf("order = [%q, %q], want [io.EOF, os.ErrNotExist]", sentinels[0].Expr, sentinels[1].Expr)
	}
}

func TestMineErrorSentinels_RejectsNonSentinels(t *testing.T) {
	const src = `package targets

import "errors"

var errUnexported = errors.New("unexported")

func makeErr() error { return errors.New("dynamic") }

func F(err error) bool {
	local := errors.New("local")
	// second arg is a local var -> rejected
	if errors.Is(err, local) {
		return true
	}
	// second arg is a function-call result -> rejected
	if errors.Is(err, makeErr()) {
		return true
	}
	// second arg is an unexported package-level var -> rejected
	if errors.Is(err, errUnexported) {
		return true
	}
	return false
}
`
	got := mineFrom(t, src, "F", "err")
	if len(got) != 0 {
		t.Fatalf("expected no sentinels mined, got: %+v", got)
	}
}

func TestMineErrorSentinels_RejectsShadowedErrorsPackage(t *testing.T) {
	// A method call `shadow.Is(err, io.EOF)` on a non-errors receiver must not
	// be mistaken for the stdlib errors.Is.
	const src = `package targets

import "io"

type checker struct{}

func (checker) Is(err error, target error) bool { return false }

func F(err error) bool {
	var errors checker
	return errors.Is(err, io.EOF)
}
`
	got := mineFrom(t, src, "F", "err")
	if len(got) != 0 {
		t.Fatalf("shadowed errors.Is must mine nothing, got: %+v", got)
	}
}

func TestMineErrorSentinels_ErrorsAs_AddressOfLocal_Rejected(t *testing.T) {
	const src = `package targets

import (
	"errors"
	"os"
)

func F(err error) bool {
	var pe *os.PathError
	return errors.As(err, &pe)
}
`
	got := mineFrom(t, src, "F", "err")
	if len(got) != 0 {
		t.Fatalf("errors.As on &localVar must mine nothing, got: %+v", got)
	}
}

func TestMineErrorSentinels_FirstArgMustBeErrorParam(t *testing.T) {
	// errors.Is whose first arg is a different (non-param) error is not keyed to
	// the param, so nothing is mined for "err".
	const src = `package targets

import (
	"errors"
	"io"
)

func F(err error) bool {
	other := errors.New("other")
	return errors.Is(other, io.EOF)
}
`
	got := mineFrom(t, src, "F", "err")
	if len(got) != 0 {
		t.Fatalf("expected nothing keyed to err, got: %+v", got)
	}
}

func TestMineErrorSentinels_CapAt16(t *testing.T) {
	src := "package targets\n\nimport \"errors\"\n\n"
	for i := range 20 {
		src += fmt.Sprintf("var Err%d = errors.New(\"e%d\")\n", i, i)
	}
	src += "\nfunc F(err error) int {\n"
	for i := range 20 {
		src += fmt.Sprintf("\tif errors.Is(err, Err%d) {\n\t\treturn %d\n\t}\n", i, i)
	}
	src += "\treturn -1\n}\n"

	got := mineFrom(t, src, "F", "err")
	sentinels := got["err"]
	if len(sentinels) != MaxErrorSentinelsPerParam {
		t.Fatalf("len(sentinels) = %d, want cap %d", len(sentinels), MaxErrorSentinelsPerParam)
	}
	// The retained entries are the first 16 in source order.
	for i, s := range sentinels {
		want := fmt.Sprintf("Err%d", i)
		if s.Expr != want {
			t.Errorf("sentinels[%d].Expr = %q, want %q", i, s.Expr, want)
		}
	}
}

func TestMineErrorSentinels_NilBodyOrNoParams(t *testing.T) {
	if got := MineErrorSentinels(nil, &types.Info{}, "targets", map[string]bool{"err": true}); got != nil {
		t.Errorf("nil body: got %+v, want nil", got)
	}
	// A real body but empty error-param set mines nothing.
	got := mineFrom(t, `package targets

import (
	"errors"
	"io"
)

func F(err error) bool { return errors.Is(err, io.EOF) }
`, "F") // no error param names supplied
	if got != nil {
		t.Errorf("empty param set: got %+v, want nil", got)
	}
}

// TestBuildWrapperTargets_MinesErrorSentinels closes the loop through the real
// wrapper build path: BuildWrapperTargets must attach the mined sentinels to the
// error parameter's WrapperParam and thread the imported sentinel's package path
// into the target's Imports (str-kvzh7).
func TestBuildWrapperTargets_MinesErrorSentinels(t *testing.T) {
	const src = `package targets

import (
	"errors"
	"io"
)

var ErrLocal = errors.New("local")

func Handle(err error) string {
	if errors.Is(err, io.EOF) {
		return "eof"
	}
	if errors.Is(err, ErrLocal) {
		return "local"
	}
	return "other"
}
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "src.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Defs: map[*ast.Ident]types.Object{},
		Uses: map[*ast.Ident]types.Object{},
	}
	conf := types.Config{Importer: importer.Default()}
	tpkg, err := conf.Check("targets", fset, []*ast.File{file}, info)
	if err != nil {
		t.Fatalf("type-check: %v", err)
	}
	pkg := &packages.Package{
		Name:      "targets",
		PkgPath:   "targets",
		Syntax:    []*ast.File{file},
		Types:     tpkg,
		TypesInfo: info,
	}

	targets := BuildWrapperTargets(pkg)
	if len(targets) != 1 {
		t.Fatalf("expected 1 target, got %d", len(targets))
	}
	got := targets[0]
	if len(got.Parameters) != 1 {
		t.Fatalf("expected 1 param, got %d", len(got.Parameters))
	}
	sentinels := got.Parameters[0].ErrorSentinels
	if len(sentinels) != 2 {
		t.Fatalf("ErrorSentinels = %+v, want 2", sentinels)
	}
	if sentinels[0].Expr != "io.EOF" || sentinels[0].ImportPath != "io" {
		t.Errorf("sentinels[0] = %+v, want {io.EOF, io}", sentinels[0])
	}
	if sentinels[1].Expr != "ErrLocal" || sentinels[1].ImportPath != "" {
		t.Errorf("sentinels[1] = %+v, want {ErrLocal, <same-package>}", sentinels[1])
	}
	if !slices.Contains(got.Imports, "io") {
		t.Errorf("target.Imports missing io; got: %v", got.Imports)
	}
}

// Property: mining is deterministic and never exceeds the cap. Two independent
// mines of the same source produce identical results, and no run returns more
// than MaxErrorSentinelsPerParam sentinels for any param.
func TestProperty_MineErrorSentinels_DeterministicAndCapped(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		n := rapid.IntRange(0, 25).Draw(rt, "sentinelCount")
		// A baseline errors use keeps the import live even when n == 0.
		src := "package targets\n\nimport \"errors\"\n\nvar _ = errors.New(\"base\")\n\n"
		for i := range n {
			src += fmt.Sprintf("var Err%d = errors.New(\"e%d\")\n", i, i)
		}
		src += "\nfunc F(err error) int {\n"
		for i := range n {
			src += fmt.Sprintf("\tif errors.Is(err, Err%d) { return %d }\n", i, i)
		}
		src += "\treturn -1\n}\n"

		fset := token.NewFileSet()
		file, err := parser.ParseFile(fset, "src.go", src, 0)
		if err != nil {
			rt.Fatalf("parse: %v", err)
		}
		mk := func() map[string][]ErrorSentinel {
			info := &types.Info{
				Defs: map[*ast.Ident]types.Object{},
				Uses: map[*ast.Ident]types.Object{},
			}
			conf := types.Config{Importer: importer.Default()}
			if _, err := conf.Check("targets", fset, []*ast.File{file}, info); err != nil {
				rt.Fatalf("type-check: %v", err)
			}
			var fn *ast.FuncDecl
			for _, decl := range file.Decls {
				if f, ok := decl.(*ast.FuncDecl); ok && f.Name.Name == "F" {
					fn = f
				}
			}
			return MineErrorSentinels(fn.Body, info, "targets", map[string]bool{"err": true})
		}
		first := mk()
		second := mk()

		wantLen := min(n, MaxErrorSentinelsPerParam)
		if len(first["err"]) != wantLen {
			rt.Fatalf("len = %d, want %d (n=%d, cap=%d)", len(first["err"]), wantLen, n, MaxErrorSentinelsPerParam)
		}
		if len(first["err"]) > MaxErrorSentinelsPerParam {
			rt.Fatalf("cap violated: %d", len(first["err"]))
		}
		if len(first["err"]) != len(second["err"]) {
			rt.Fatalf("nondeterministic length: %d vs %d", len(first["err"]), len(second["err"]))
		}
		for i := range first["err"] {
			if first["err"][i] != second["err"][i] {
				rt.Fatalf("nondeterministic at %d: %+v vs %+v", i, first["err"][i], second["err"][i])
			}
		}
	})
}
