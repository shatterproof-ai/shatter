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

// TestBuildWrapperTargets_DoesNotImportResultOnlyPackages guards against
// package-wide wrapper build failures from unused imports. The generated
// wrapper never names result types, so result-only selector packages must not
// be emitted in the wrapper import block.
func TestBuildWrapperTargets_DoesNotImportResultOnlyPackages(t *testing.T) {
	const src = `package resultonly

import "time"

func Wait() time.Duration { return 0 }
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "resultonly.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Defs:  map[*ast.Ident]types.Object{},
		Uses:  map[*ast.Ident]types.Object{},
		Types: map[ast.Expr]types.TypeAndValue{},
	}
	conf := types.Config{Importer: importer.Default()}
	tpkg, err := conf.Check("resultonly", fset, []*ast.File{file}, info)
	if err != nil {
		t.Fatalf("type-check: %v", err)
	}
	pkg := &packages.Package{
		Name:      "resultonly",
		PkgPath:   "example.com/resultonly",
		Syntax:    []*ast.File{file},
		Types:     tpkg,
		TypesInfo: info,
	}

	targets := BuildWrapperTargets(pkg)
	if len(targets) != 1 {
		t.Fatalf("expected 1 target, got %d", len(targets))
	}
	if slices.Contains(targets[0].Imports, "time") {
		t.Fatalf("result-only import leaked into wrapper target imports: %+v", targets[0].Imports)
	}

	out := GenerateWrapper("resultonly", targets, nil)
	if strings.Contains(out, `"time"`) {
		t.Fatalf("generated wrapper contains unused result-only import:\n%s", out)
	}
}

func keysOf(m map[string]struct{}) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	return out
}

// TestBuildWrapperTargets_ExcludesSyntheticInit (str-qo1.8) verifies that
// `func init()` declarations across multiple files in the same package are
// excluded from the wrapper target list. Without the fix, a package with N
// init functions produced N WrapperTargets all sharing the ID
// "<pkgPath>:init", which generated a wrapper file with duplicate switch
// cases that calls init() directly — both Go-illegal.
func TestBuildWrapperTargets_ExcludesSyntheticInit(t *testing.T) {
	const fileA = `package multinit

var seedA int

func init() { seedA = 1 }

func Hello() string { return "hi" }
`
	const fileB = `package multinit

var seedB int

func init() { seedB = 2 }

func init() { seedB += 10 }
`
	const fileC = `package multinit

func init() { seedA += seedB }
`
	fset := token.NewFileSet()
	parsed := make([]*ast.File, 0, 3)
	for _, src := range []string{fileA, fileB, fileC} {
		file, err := parser.ParseFile(fset, "", src, 0)
		if err != nil {
			t.Fatalf("parse: %v", err)
		}
		parsed = append(parsed, file)
	}
	info := &types.Info{
		Defs: map[*ast.Ident]types.Object{},
		Uses: map[*ast.Ident]types.Object{},
	}
	conf := types.Config{Importer: importer.Default()}
	tpkg, err := conf.Check("multinit", fset, parsed, info)
	if err != nil {
		t.Fatalf("type-check: %v", err)
	}
	pkg := &packages.Package{
		Name:      "multinit",
		PkgPath:   "example.com/multinit",
		Syntax:    parsed,
		Types:     tpkg,
		TypesInfo: info,
	}

	targets := BuildWrapperTargets(pkg)

	// No init targets and no duplicate IDs.
	seen := make(map[string]int)
	for _, target := range targets {
		if strings.HasSuffix(target.ID, ":init") {
			t.Errorf("BuildWrapperTargets surfaced synthetic init target: %q", target.ID)
		}
		seen[target.ID]++
	}
	for id, n := range seen {
		if n > 1 {
			t.Errorf("duplicate WrapperTarget ID %q (count=%d)", id, n)
		}
	}
	if len(targets) != 1 || targets[0].SymbolName != "Hello" {
		t.Errorf("expected single Hello target, got: %+v", targets)
	}
}

// TestGenerateWrapper_NoInitSwitchCaseFromAST (str-qo1.8) closes the loop
// at the source-generation level: the wrapper produced from a package with
// multiple init declarations must not contain a `case "...:init":` line and
// must not call init() directly.
func TestGenerateWrapper_NoInitSwitchCaseFromAST(t *testing.T) {
	const src = `package multinit

func init() {}
func init() {}
func Hello() string { return "" }
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Defs: map[*ast.Ident]types.Object{},
		Uses: map[*ast.Ident]types.Object{},
	}
	conf := types.Config{Importer: importer.Default()}
	tpkg, err := conf.Check("multinit", fset, []*ast.File{file}, info)
	if err != nil {
		t.Fatalf("type-check: %v", err)
	}
	pkg := &packages.Package{
		Name:      "multinit",
		PkgPath:   "example.com/multinit",
		Syntax:    []*ast.File{file},
		Types:     tpkg,
		TypesInfo: info,
	}

	targets := BuildWrapperTargets(pkg)
	out := GenerateWrapper("multinit", targets, nil)

	if strings.Contains(out, ":init\"") {
		t.Errorf("generated wrapper contains an init switch case:\n%s", out)
	}
	if strings.Contains(out, "init()") {
		t.Errorf("generated wrapper calls init() directly:\n%s", out)
	}
}

// TestExtractWrapperParams_SynthesizesNamesForUnnamedAndBlank is the
// str-qo1.7 unit-level regression: when a parameter has no name
// (e.g. `func F(int, string)`) or uses the blank identifier
// (e.g. `func F(_ int, _ string)`), the wrapper-local name must be a
// stable synthetic identifier — not the empty string and not "_" —
// because the wrapper later references each name in
// `json.Unmarshal(&p)` and in the call expression. Emitting `_` would
// produce "cannot use _ as value or type" at build time.
func TestExtractWrapperParams_SynthesizesNamesForUnnamedAndBlank(t *testing.T) {
	const src = `package x

import "go/token"

func Unnamed(int, string) {}
func Blank(_ int, _ string) {}
func Mixed(a int, _ string, _ token.Pos) {}
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "x.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Defs:  map[*ast.Ident]types.Object{},
		Uses:  map[*ast.Ident]types.Object{},
		Types: map[ast.Expr]types.TypeAndValue{},
	}
	conf := types.Config{Importer: importer.Default()}
	if _, err := conf.Check("x", fset, []*ast.File{file}, info); err != nil {
		t.Fatalf("type-check: %v", err)
	}

	byName := make(map[string]*ast.FuncDecl)
	for _, decl := range file.Decls {
		if fn, ok := decl.(*ast.FuncDecl); ok {
			byName[fn.Name.Name] = fn
		}
	}

	cases := []struct {
		funcName  string
		wantNames []string
	}{
		{"Unnamed", []string{"_p0", "_p1"}},
		{"Blank", []string{"_p0", "_p1"}},
		{"Mixed", []string{"a", "_p1", "_p2"}},
	}
	for _, tc := range cases {
		fn := byName[tc.funcName]
		if fn == nil {
			t.Fatalf("decl %s not found", tc.funcName)
		}
		params := extractWrapperParams(fn, info, "x", nil)
		if len(params) != len(tc.wantNames) {
			t.Errorf("%s: got %d params, want %d (%v)", tc.funcName, len(params), len(tc.wantNames), params)
			continue
		}
		for i, want := range tc.wantNames {
			if params[i].Name != want {
				t.Errorf("%s: param[%d].Name = %q, want %q", tc.funcName, i, params[i].Name, want)
			}
			if params[i].Name == "_" || params[i].Name == "" {
				t.Errorf("%s: param[%d].Name = %q is unusable as a Go identifier", tc.funcName, i, params[i].Name)
			}
		}
	}
}
