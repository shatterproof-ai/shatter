package wrapper

import (
	"go/ast"
	"go/importer"
	"go/parser"
	"go/token"
	"go/types"
	"slices"
	"strings"
	"testing"

	"golang.org/x/tools/go/packages"
)

// TestWrapperGoType_SelectorImportRecordedOnASTFallback is the str-qo1.13
// regression: when info.Types lacks an entry for a selector type expression
// (e.g. http.ResponseWriter) and wrapperGoType falls back to AST printing,
// the corresponding import path must still be recorded into importSet via
// info.Uses. Without the fix, the generated wrapper text contains
// `http.ResponseWriter` without importing `net/http` and fails to compile
// with `undefined: http`.
func TestWrapperGoType_SelectorImportRecordedOnASTFallback(t *testing.T) {
	const src = `package handlers

import "net/http"

func Handle(w http.ResponseWriter, r *http.Request) {}
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "h.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}

	// Deliberately omit Types from the Info to simulate the bug repro: the
	// type checker has Uses populated (which is enough to identify the
	// imported package) but Types[expr] is empty, forcing the AST fallback
	// in wrapperGoType. This mirrors the production failure shape described
	// in str-qo1.13 where type info falls back to AST printing.
	info := &types.Info{
		Defs: map[*ast.Ident]types.Object{},
		Uses: map[*ast.Ident]types.Object{},
	}
	conf := types.Config{Importer: importer.Default()}
	if _, err := conf.Check("handlers", fset, []*ast.File{file}, info); err != nil {
		t.Fatalf("type-check: %v", err)
	}

	// Locate the FuncDecl and its parameter type expressions.
	var fn *ast.FuncDecl
	for _, decl := range file.Decls {
		if f, ok := decl.(*ast.FuncDecl); ok && f.Name.Name == "Handle" {
			fn = f
			break
		}
	}
	if fn == nil {
		t.Fatalf("Handle decl not found")
	}

	importSet := make(map[string]struct{})
	for _, field := range fn.Type.Params.List {
		_ = wrapperGoType(field.Type, info, "handlers", importSet)
	}

	if _, ok := importSet["net/http"]; !ok {
		t.Errorf("importSet missing net/http after AST fallback; got: %v", keysOf(importSet))
	}
}

// TestBuildWrapperTargets_SelectorImportFallback exercises the bug end-to-end
// through BuildWrapperTargets: a same-package function using
// http.ResponseWriter must emit `net/http` in the resulting WrapperTarget's
// Imports even when the loaded *types.Info lacks Types entries for the
// selector type expressions. (str-qo1.13 regression for the zolem
// internal/response/faker.go:tokenizeWords compile failure.)
func TestBuildWrapperTargets_SelectorImportFallback(t *testing.T) {
	const src = `package handlers

import "net/http"

func Handle(w http.ResponseWriter, r *http.Request) {}
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "h.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Defs: map[*ast.Ident]types.Object{},
		Uses: map[*ast.Ident]types.Object{},
	}
	conf := types.Config{Importer: importer.Default()}
	tpkg, err := conf.Check("handlers", fset, []*ast.File{file}, info)
	if err != nil {
		t.Fatalf("type-check: %v", err)
	}

	pkg := &packages.Package{
		Name:      "handlers",
		PkgPath:   "example.com/handlers",
		Syntax:    []*ast.File{file},
		Types:     tpkg,
		TypesInfo: info,
	}

	targets := BuildWrapperTargets(pkg)
	if len(targets) != 1 {
		t.Fatalf("expected 1 target, got %d", len(targets))
	}
	got := targets[0]
	if !slices.Contains(got.Imports, "net/http") {
		t.Errorf("target.Imports missing net/http; got: %v", got.Imports)
	}
}

// TestGenerateWrapper_SelectorTypeFromASTFallbackEmitsImport closes the loop
// at the source-generation level: a wrapper built from BuildWrapperTargets'
// fallback path must contain an `import "net/http"` line in addition to the
// http.ResponseWriter parameter declaration. (str-qo1.13)
func TestGenerateWrapper_SelectorTypeFromASTFallbackEmitsImport(t *testing.T) {
	const src = `package handlers

import "net/http"

func Handle(w http.ResponseWriter, r *http.Request) {}
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "h.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Defs: map[*ast.Ident]types.Object{},
		Uses: map[*ast.Ident]types.Object{},
	}
	conf := types.Config{Importer: importer.Default()}
	tpkg, err := conf.Check("handlers", fset, []*ast.File{file}, info)
	if err != nil {
		t.Fatalf("type-check: %v", err)
	}
	pkg := &packages.Package{
		Name:      "handlers",
		PkgPath:   "example.com/handlers",
		Syntax:    []*ast.File{file},
		Types:     tpkg,
		TypesInfo: info,
	}

	targets := BuildWrapperTargets(pkg)
	out := GenerateWrapper("handlers", targets, nil)

	if !strings.Contains(out, `"net/http"`) {
		t.Errorf("generated wrapper missing net/http import; source:\n%s", out)
	}
	if !strings.Contains(out, "http.ResponseWriter") {
		t.Errorf("generated wrapper missing http.ResponseWriter param; source:\n%s", out)
	}
}

func keysOf(m map[string]struct{}) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	return out
}
