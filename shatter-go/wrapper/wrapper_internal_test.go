package wrapper

import (
	"fmt"
	"go/ast"
	"go/importer"
	"go/parser"
	"go/token"
	"go/types"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"

	"golang.org/x/tools/go/packages"

	"github.com/shatter-dev/shatter/shatter-go/config"
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
		_ = wrapperGoType(field.Type, info, "handlers", "", importSet)
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

func TestGenerateWrapper_InitializedMapsReceiverKind(t *testing.T) {
	target := WrapperTarget{
		ID:            "example.com/fixture:(*MapOnlyBuilder).Mark",
		SymbolName:    "Mark",
		Kind:          TargetKindMethod,
		ReceiverType:  "MapOnlyBuilder",
		IsPointerRecv: true,
		ReceiverMapFields: []ReceiverMapField{{
			Name:   "seen",
			GoType: "map[string]bool",
		}},
		HasResult:     true,
		ResultGoType:  "int",
		ResultGoTypes: []string{"int"},
		ResultCount:   1,
	}
	out := GenerateWrapper("fixture", []WrapperTarget{target}, nil)

	if !strings.Contains(out, `case "initialized_maps":`) {
		t.Fatalf("generated wrapper missing initialized_maps case; source:\n%s", out)
	}
	if !strings.Contains(out, "seen: map[string]bool{},") {
		t.Fatalf("generated wrapper missing initialized map field; source:\n%s", out)
	}
	if !strings.Contains(out, "_recv := &_recvVal") {
		t.Fatalf("generated wrapper missing pointer receiver binding; source:\n%s", out)
	}
}

func TestBuildWrapperTargets_InitializedMapsSkipsNonMapHiddenState(t *testing.T) {
	const src = `package fixture

type MixedReceiver struct {
	seen map[string]bool
	done chan struct{}
}

func (m *MixedReceiver) Mark() int {
	m.seen["x"] = true
	return len(m.seen)
}
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "mixed.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Defs: map[*ast.Ident]types.Object{},
		Uses: map[*ast.Ident]types.Object{},
	}
	conf := types.Config{Importer: importer.Default()}
	tpkg, err := conf.Check("fixture", fset, []*ast.File{file}, info)
	if err != nil {
		t.Fatalf("type-check: %v", err)
	}
	pkg := &packages.Package{
		Name:      "fixture",
		PkgPath:   "example.com/fixture",
		Syntax:    []*ast.File{file},
		Types:     tpkg,
		TypesInfo: info,
	}

	targets := BuildWrapperTargets(pkg)
	if len(targets) != 1 {
		t.Fatalf("expected 1 target, got %d", len(targets))
	}
	if len(targets[0].ReceiverMapFields) != 0 {
		t.Fatalf("ReceiverMapFields = %+v, want nil for receiver with hidden channel state", targets[0].ReceiverMapFields)
	}
	out := GenerateWrapper("fixture", targets, nil)
	if strings.Contains(out, `case "initialized_maps":`) {
		t.Fatalf("generated wrapper should not expose initialized_maps for hidden channel state; source:\n%s", out)
	}
}

// str-gxjs.1: when a parameter's Go-source type matches a runtime-value
// registry entry (context.Context, http.ResponseWriter, …), the generated
// wrapper must (a) substitute the registered expression at the param-init
// site instead of decoding from inputs[i] via json.Unmarshal, and
// (b) emit the import paths the expression needs. Without this, a function
// taking context.Context would compile-link but leave the param as the
// zero interface value (nil), panicking on first use.
func TestGenerateWrapper_RuntimeValueSubstitutesContextBackground(t *testing.T) {
	const src = `package svc

import "context"

func Ping(ctx context.Context) int { return 1 }
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "h.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Defs:  map[*ast.Ident]types.Object{},
		Uses:  map[*ast.Ident]types.Object{},
		Types: map[ast.Expr]types.TypeAndValue{},
	}
	conf := types.Config{Importer: importer.Default()}
	tpkg, err := conf.Check("svc", fset, []*ast.File{file}, info)
	if err != nil {
		t.Fatalf("type-check: %v", err)
	}
	pkg := &packages.Package{
		Name:      "svc",
		PkgPath:   "example.com/svc",
		Syntax:    []*ast.File{file},
		Types:     tpkg,
		TypesInfo: info,
	}

	targets := BuildWrapperTargets(pkg)
	if len(targets) != 1 {
		t.Fatalf("len(targets) = %d, want 1", len(targets))
	}
	target := targets[0]
	if len(target.Parameters) != 1 {
		t.Fatalf("len(parameters) = %d, want 1", len(target.Parameters))
	}
	if target.Parameters[0].RuntimeValueExpr != "context.Background()" {
		t.Errorf("RuntimeValueExpr = %q, want %q",
			target.Parameters[0].RuntimeValueExpr, "context.Background()")
	}
	if !slices.Contains(target.Imports, "context") {
		t.Errorf("target.Imports = %v, want to contain %q", target.Imports, "context")
	}

	out := GenerateWrapper("svc", targets, nil)
	if !strings.Contains(out, "var ctx context.Context = context.Background()") {
		t.Errorf("wrapper missing direct context.Background() assignment; source:\n%s", out)
	}
	if strings.Contains(out, "json.Unmarshal(inputs[0], &ctx)") {
		t.Errorf("wrapper still decodes ctx from inputs; runtime-value substitution should bypass json.Unmarshal; source:\n%s", out)
	}
}

func TestGenerateWrapper_RuntimeValueSubstitutesTemplate(t *testing.T) {
	const src = `package svc

import "text/template"

func Render(t *template.Template) error { return t.Execute(nil, nil) }
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "h.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Defs:  map[*ast.Ident]types.Object{},
		Uses:  map[*ast.Ident]types.Object{},
		Types: map[ast.Expr]types.TypeAndValue{},
	}
	conf := types.Config{Importer: importer.Default()}
	tpkg, err := conf.Check("svc", fset, []*ast.File{file}, info)
	if err != nil {
		t.Fatalf("type-check: %v", err)
	}
	pkg := &packages.Package{
		Name:      "svc",
		PkgPath:   "example.com/svc",
		Syntax:    []*ast.File{file},
		Types:     tpkg,
		TypesInfo: info,
	}

	targets := BuildWrapperTargets(pkg)
	if len(targets) != 1 {
		t.Fatalf("len(targets) = %d, want 1", len(targets))
	}
	target := targets[0]
	if got := target.Parameters[0].RuntimeValueExpr; got != `template.Must(template.New("shatter").Parse("{}"))` {
		t.Fatalf("RuntimeValueExpr = %q", got)
	}
	if !slices.Contains(target.Imports, "text/template") {
		t.Fatalf("target.Imports = %v, want text/template", target.Imports)
	}

	out := GenerateWrapper("svc", targets, nil)
	if !strings.Contains(out, `var t *template.Template = template.Must(template.New("shatter").Parse("{}"))`) {
		t.Errorf("wrapper missing template runtime assignment; source:\n%s", out)
	}
	if strings.Contains(out, "json.Unmarshal(inputs[0], &t)") {
		t.Errorf("wrapper still decodes template from inputs; source:\n%s", out)
	}
}

func TestBuildWrapperTargets_ImportedValueParamUsesParameterlessConstructor(t *testing.T) {
	modDir := t.TempDir()
	depDir := filepath.Join(modDir, "dep")
	appDir := filepath.Join(modDir, "app")
	if err := os.MkdirAll(depDir, 0o755); err != nil {
		t.Fatalf("mkdir dep: %v", err)
	}
	if err := os.MkdirAll(appDir, 0o755); err != nil {
		t.Fatalf("mkdir app: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/ctorparam\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}
	const depSrc = `package dep

type Client struct {
	baseURL string
}

func NewClient(baseURL string) Client { return Client{baseURL: baseURL} }
func NewInertClient() Client { return Client{baseURL: "http://127.0.0.1:0"} }
func NewPointerClient() *Client { return &Client{} }
func NewErrorClient() (Client, error) { return Client{}, nil }

func (c Client) BaseURL() string { return c.baseURL }
`
	if err := os.WriteFile(filepath.Join(depDir, "dep.go"), []byte(depSrc), 0o644); err != nil {
		t.Fatalf("write dep.go: %v", err)
	}
	const appSrc = `package app

import safe "example.com/ctorparam/dep"

func Use(client safe.Client) string {
	return client.BaseURL()
}
`
	if err := os.WriteFile(filepath.Join(appDir, "app.go"), []byte(appSrc), 0o644); err != nil {
		t.Fatalf("write app.go: %v", err)
	}

	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedSyntax |
			packages.NeedTypes | packages.NeedTypesInfo |
			packages.NeedCompiledGoFiles | packages.NeedImports | packages.NeedDeps,
		Dir: appDir,
		Env: append(os.Environ(), "GOFLAGS="),
	}
	pkgs, err := packages.Load(cfg, ".")
	if err != nil {
		t.Fatalf("packages.Load: %v", err)
	}
	if len(pkgs) != 1 {
		t.Fatalf("loaded %d packages, want 1", len(pkgs))
	}
	for _, pkgErr := range pkgs[0].Errors {
		t.Fatalf("package load error: %v", pkgErr)
	}

	targets := BuildWrapperTargets(pkgs[0])
	if len(targets) != 1 {
		t.Fatalf("BuildWrapperTargets produced %d targets, want 1", len(targets))
	}
	target := targets[0]
	if len(target.Parameters) != 1 {
		t.Fatalf("Use param count = %d, want 1", len(target.Parameters))
	}
	if got, want := target.Parameters[0].RuntimeValueExpr, "dep.NewInertClient()"; got != want {
		t.Fatalf("RuntimeValueExpr = %q, want %q", got, want)
	}
	if !slices.Contains(target.Imports, "example.com/ctorparam/dep") {
		t.Fatalf("target.Imports = %v, want imported dep package", target.Imports)
	}

	out := GenerateWrapper("app", targets, nil)
	if !strings.Contains(out, `"example.com/ctorparam/dep"`) {
		t.Fatalf("generated wrapper missing dep import:\n%s", out)
	}
	if !strings.Contains(out, "var client dep.Client = dep.NewInertClient()") {
		t.Fatalf("generated wrapper missing constructor-backed client assignment:\n%s", out)
	}
	if strings.Contains(out, "json.Unmarshal(_shatterInputs[0], &client)") {
		t.Fatalf("generated wrapper still decodes constructor-backed client from JSON:\n%s", out)
	}
}

func TestGenerateWrapper_RuntimeValueSubstitutesWazeroRuntime(t *testing.T) {
	importSet := make(map[string]struct{})
	params := []WrapperParam{{Name: "rt", GoType: "wazero.Runtime"}}
	applyRuntimeValueBindings(params, importSet)
	if got := params[0].RuntimeValueExpr; got != `wazero.NewRuntime(context.Background())` {
		t.Fatalf("RuntimeValueExpr = %q", got)
	}
	if _, ok := importSet["context"]; !ok {
		t.Fatalf("importSet = %v, want context", importSet)
	}
	if _, ok := importSet["github.com/tetratelabs/wazero"]; !ok {
		t.Fatalf("importSet = %v, want github.com/tetratelabs/wazero", importSet)
	}

	targets := []WrapperTarget{{
		ID:         "example.com/svc:UseRuntime",
		SymbolName: "UseRuntime",
		Kind:       TargetKindFunction,
		Parameters: params,
		Imports:    keysOf(importSet),
	}}
	out := GenerateWrapper("svc", targets, nil)
	if !strings.Contains(out, `var rt wazero.Runtime = wazero.NewRuntime(context.Background())`) {
		t.Errorf("wrapper missing wazero runtime assignment; source:\n%s", out)
	}
	if strings.Contains(out, "json.Unmarshal(inputs[0], &rt)") {
		t.Errorf("wrapper still decodes wazero runtime from inputs; source:\n%s", out)
	}
}

func TestGenerateWrapper_RuntimeValueSubstitutesWazeroCompiledModule(t *testing.T) {
	importSet := make(map[string]struct{})
	params := []WrapperParam{{Name: "compiled", GoType: "wazero.CompiledModule"}}
	applyRuntimeValueBindings(params, importSet)
	if got := params[0].RuntimeValueExpr; !strings.Contains(got, `func() wazero.CompiledModule`) {
		t.Fatalf("RuntimeValueExpr = %q, want compiled module expression", got)
	}
	if _, ok := importSet["context"]; !ok {
		t.Fatalf("importSet = %v, want context", importSet)
	}
	if _, ok := importSet["github.com/tetratelabs/wazero"]; !ok {
		t.Fatalf("importSet = %v, want github.com/tetratelabs/wazero", importSet)
	}

	targets := []WrapperTarget{{
		ID:         "example.com/svc:UseCompiledModule",
		SymbolName: "UseCompiledModule",
		Kind:       TargetKindFunction,
		Parameters: params,
		Imports:    keysOf(importSet),
	}}
	out := GenerateWrapper("svc", targets, nil)
	if !strings.Contains(out, `var compiled wazero.CompiledModule = func() wazero.CompiledModule`) {
		t.Errorf("wrapper missing wazero compiled module assignment; source:\n%s", out)
	}
	if strings.Contains(out, "json.Unmarshal(inputs[0], &compiled)") {
		t.Errorf("wrapper still decodes wazero compiled module from inputs; source:\n%s", out)
	}
}

func TestGenerateWrapper_ConfiguredRuntimeValueSubstitutesExactType(t *testing.T) {
	importSet := make(map[string]struct{})
	params := []WrapperParam{{Name: "mod", GoType: "fixture.CompiledModule"}}
	expr := `func() fixture.CompiledModule { return fixture.CompiledModule{} }()`
	applyRuntimeValueBindings(params, importSet, map[string]config.GoRuntimeValueConfig{
		"fixture.CompiledModule": {
			Expression: expr,
			Imports:    []string{"zolem.dev/zolem/internal/fixture"},
		},
	})
	if got := params[0].RuntimeValueExpr; got != expr {
		t.Fatalf("RuntimeValueExpr = %q, want %q", got, expr)
	}
	if _, ok := importSet["zolem.dev/zolem/internal/fixture"]; !ok {
		t.Fatalf("importSet = %v, want configured fixture import", importSet)
	}

	targets := []WrapperTarget{{
		ID:         "example.com/svc:UseModule",
		SymbolName: "UseModule",
		Kind:       TargetKindFunction,
		Parameters: params,
		Imports:    keysOf(importSet),
	}}
	out := GenerateWrapper("svc", targets, nil)
	if !strings.Contains(out, `var mod fixture.CompiledModule = func() fixture.CompiledModule { return fixture.CompiledModule{} }()`) {
		t.Errorf("wrapper missing configured runtime assignment; source:\n%s", out)
	}
	if strings.Contains(out, "json.Unmarshal(_shatterInputs[0], &mod)") {
		t.Errorf("wrapper still decodes configured runtime value from inputs; source:\n%s", out)
	}
}

func TestApplyRuntimeValueBindings_ConfiguredSamePackageType(t *testing.T) {
	params := []WrapperParam{{Name: "mod", GoType: "CompiledModule"}}
	expr := `func() CompiledModule { return CompiledModule{} }()`
	applyRuntimeValueBindingsForPackage(params, map[string]struct{}{}, map[string]config.GoRuntimeValueConfig{
		"fixture.CompiledModule": {Expression: expr},
	}, "fixture")
	if params[0].RuntimeValueExpr != expr {
		t.Fatalf("RuntimeValueExpr = %q, want same-package configured expression", params[0].RuntimeValueExpr)
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

// TestExtractWrapperParams_PreservesPointerShape (str-9j2e) verifies that a
// parameter declared as a single pointer (`*T`) emits a wrapper local typed
// as `*T`, not `[]*T`. Pre-fix the zolem `internal/provider/openai/handler.go::NewHandler`
// build failed with `cannot use wasmGenerator (variable of type []*wasmgen.Generator)
// as *wasmgen.Generator value` because the wrapper's variable declaration
// was being prefixed with `[]` even though the parameter was not variadic.
func TestExtractWrapperParams_PreservesPointerShape(t *testing.T) {
	const src = `package handlers

type Generator struct{}
type Handler struct{}

func NewHandler(gen *Generator) *Handler { return &Handler{} }
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "h.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Types: map[ast.Expr]types.TypeAndValue{},
		Defs:  map[*ast.Ident]types.Object{},
		Uses:  map[*ast.Ident]types.Object{},
	}
	conf := types.Config{Importer: importer.Default()}
	tpkg, err := conf.Check("handlers", fset, []*ast.File{file}, info)
	if err != nil {
		t.Fatalf("type-check: %v", err)
	}
	pkg := &packages.Package{
		Name:    "handlers",
		PkgPath: "example.com/handlers",
		// str-jeen.79: set Types so buildWrapperTarget can use pkg.Types.Path()
		// ("handlers") for the qualifier comparison instead of pkg.PkgPath
		// ("example.com/handlers"), which would mismatch the type checker's path.
		Types:     tpkg,
		Syntax:    []*ast.File{file},
		TypesInfo: info,
	}

	targets := BuildWrapperTargets(pkg)
	var newHandler *WrapperTarget
	for i, target := range targets {
		if target.SymbolName == "NewHandler" {
			newHandler = &targets[i]
			break
		}
	}
	if newHandler == nil {
		t.Fatalf("NewHandler target not found; targets: %+v", targets)
	}
	if len(newHandler.Parameters) != 1 {
		t.Fatalf("expected 1 parameter, got %d: %+v", len(newHandler.Parameters), newHandler.Parameters)
	}
	param := newHandler.Parameters[0]
	if param.IsVariadic {
		t.Errorf("param IsVariadic = true for a non-variadic pointer parameter; want false")
	}
	if param.GoType != "*Generator" {
		t.Errorf("param.GoType = %q, want %q (callers see a slice type and trip 'cannot use ... as *Generator value')", param.GoType, "*Generator")
	}
}

// TestGenerateWrapper_ValueReturningConstructor (str-9j2e) verifies that a
// value-returning constructor combined with a value-receiver method emits
// `_recv := DefaultRegistry()` rather than `_recv := *DefaultRegistry()`.
// The latter caused the zolem `internal/specs/registry.go::DefaultRegistry`
// build failure `cannot indirect DefaultRegistry() (value of struct type
// Registry)` because the wrapper applied a pointer dereference to a
// value-typed expression.
func TestGenerateWrapper_ValueReturningConstructor(t *testing.T) {
	const src = `package specs

type Registry struct{}

func DefaultRegistry() Registry { return Registry{} }

func (r Registry) DoIt() {}
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "r.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Types: map[ast.Expr]types.TypeAndValue{},
		Defs:  map[*ast.Ident]types.Object{},
		Uses:  map[*ast.Ident]types.Object{},
	}
	conf := types.Config{Importer: importer.Default()}
	if _, err := conf.Check("specs", fset, []*ast.File{file}, info); err != nil {
		t.Fatalf("type-check: %v", err)
	}
	pkg := &packages.Package{
		Name:      "specs",
		PkgPath:   "example.com/specs",
		Syntax:    []*ast.File{file},
		TypesInfo: info,
	}

	targets := BuildWrapperTargets(pkg)
	ctorCandidates := []ConstructorCandidate{
		{
			FuncName:       "DefaultRegistry",
			TargetType:     "Registry",
			HasParams:      false,
			ReturnsPointer: false, // value-returning constructor
		},
	}

	out := GenerateWrapper("specs", targets, ctorCandidates)
	if strings.Contains(out, "*DefaultRegistry()") {
		t.Errorf("wrapper applies pointer dereference to value-returning constructor:\n%s", out)
	}
	if !strings.Contains(out, "_recv := DefaultRegistry()") {
		t.Errorf("wrapper missing direct value-bind from DefaultRegistry():\n%s", out)
	}
}

// TestWrapperGoType_SiblingPackageSameNamePreservesQualifier is the str-jeen.79
// regression: when the current package and an imported sibling package share
// the same short name (e.g. both named "mcp"), the qualifier function must NOT
// strip the qualifier from sibling types. Pre-fix the comparison used
// p.Name() == pkgName, which matched same-name imports and silently dropped
// the qualifier, producing `undefined: Server` instead of `*mcp.Server`.
//
// Fix uses p.Path() == pkgPath (the full import path) so only the exact
// current package is treated as "no qualifier needed".
func TestWrapperGoType_SiblingPackageSameNamePreservesQualifier(t *testing.T) {
	// Build a sibling package "example.com/sibling/mcp" with short name "mcp"
	// that exports a Server struct. This is the sibling whose qualifier must
	// be preserved even though the current package is also named "mcp".
	siblingPkg := types.NewPackage("example.com/sibling/mcp", "mcp")
	serverTypeName := types.NewTypeName(0, siblingPkg, "Server", nil)
	serverType := types.NewNamed(serverTypeName, types.NewStruct(nil, nil), nil)
	siblingPkg.Scope().Insert(serverTypeName)

	imp := &fakeImporter{
		packages: map[string]*types.Package{
			"example.com/sibling/mcp": siblingPkg,
		},
	}

	const src = `package mcp

import extmcp "example.com/sibling/mcp"

func UseServer(s *extmcp.Server) {}
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "mcp.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}

	info := &types.Info{
		Defs:  map[*ast.Ident]types.Object{},
		Uses:  map[*ast.Ident]types.Object{},
		Types: map[ast.Expr]types.TypeAndValue{},
	}
	conf := types.Config{Importer: imp}
	_, err = conf.Check("example.com/mcp", fset, []*ast.File{file}, info)
	if err != nil {
		t.Fatalf("type-check: %v", err)
	}

	// Locate the UseServer parameter type expression.
	var fn *ast.FuncDecl
	for _, decl := range file.Decls {
		if f, ok := decl.(*ast.FuncDecl); ok && f.Name.Name == "UseServer" {
			fn = f
			break
		}
	}
	if fn == nil || fn.Type.Params == nil || len(fn.Type.Params.List) == 0 {
		t.Fatalf("UseServer not found or has no params")
	}

	importSet := make(map[string]struct{})
	// pkgName="mcp" pkgPath="example.com/mcp" — same short name as the sibling.
	typeStr := wrapperGoType(fn.Type.Params.List[0].Type, info, "mcp", "example.com/mcp", importSet)

	// The type string must preserve the package qualifier ("mcp.Server"),
	// not drop it ("Server"), even though both packages share the name "mcp".
	if typeStr == "*Server" {
		t.Errorf("wrapperGoType dropped qualifier for sibling mcp package (str-jeen.79): got %q, want %q", typeStr, "*mcp.Server")
	}
	if typeStr != "*mcp.Server" {
		t.Errorf("wrapperGoType: got %q, want %q", typeStr, "*mcp.Server")
	}

	// The sibling mcp package import path must be collected (not skipped).
	if _, ok := importSet["example.com/sibling/mcp"]; !ok {
		t.Errorf("importSet missing sibling mcp package; got: %v", keysOf(importSet))
	}

	// The current package (example.com/mcp) must NOT be in importSet.
	if _, ok := importSet["example.com/mcp"]; ok {
		t.Errorf("importSet incorrectly contains current package path example.com/mcp")
	}

	// Sanity: serverType is referenced to avoid "declared and not used".
	_ = serverType
}

// fakeImporter provides a fixed set of pre-built *types.Package values for
// use in test type-checking. It implements types.Importer.
type fakeImporter struct {
	packages map[string]*types.Package
}

func (f *fakeImporter) Import(path string) (*types.Package, error) {
	if pkg, ok := f.packages[path]; ok {
		pkg.MarkComplete()
		return pkg, nil
	}
	return nil, fmt.Errorf("package not found: %s", path)
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
		params := extractWrapperParams(fn, info, "x", "", nil)
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

// TestBuildWrapperTargets_DetectsVariadic is the str-jeen.48 analyze-side
// regression: when a function declares a final `...T` parameter,
// BuildWrapperTargets must surface IsVariadic=true on the corresponding
// WrapperParam and render the GoType as the slice form (`[]T`). Without
// this, GenerateWrapper would emit a non-expanded call site and the
// generated wrapper would fail to build.
func TestBuildWrapperTargets_DetectsVariadic(t *testing.T) {
	const src = `package vary

func RunCommand(name string, args ...string) int { return len(name) + len(args) }

func CallU32(args ...uint64) uint64 { var s uint64; for _, v := range args { s += v }; return s }

func NoVariadic(xs []string) int { return len(xs) }
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "v.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Defs:  map[*ast.Ident]types.Object{},
		Uses:  map[*ast.Ident]types.Object{},
		Types: map[ast.Expr]types.TypeAndValue{},
	}
	conf := types.Config{Importer: importer.Default()}
	tpkg, err := conf.Check("vary", fset, []*ast.File{file}, info)
	if err != nil {
		t.Fatalf("type-check: %v", err)
	}
	pkg := &packages.Package{
		Name: "vary", PkgPath: "example.com/vary",
		Syntax: []*ast.File{file}, Types: tpkg, TypesInfo: info,
	}
	targets := BuildWrapperTargets(pkg)

	byName := map[string]WrapperTarget{}
	for _, t := range targets {
		byName[t.SymbolName] = t
	}

	check := func(name, paramName, wantGoType string, wantVariadic bool) {
		t.Helper()
		tgt, ok := byName[name]
		if !ok {
			t.Fatalf("missing target %q", name)
		}
		if len(tgt.Parameters) == 0 {
			t.Fatalf("%s: no parameters", name)
		}
		last := tgt.Parameters[len(tgt.Parameters)-1]
		if last.Name != paramName {
			t.Errorf("%s: last param name = %q, want %q", name, last.Name, paramName)
		}
		if last.GoType != wantGoType {
			t.Errorf("%s: last param GoType = %q, want %q", name, last.GoType, wantGoType)
		}
		if last.IsVariadic != wantVariadic {
			t.Errorf("%s: last param IsVariadic = %v, want %v", name, last.IsVariadic, wantVariadic)
		}
	}

	check("RunCommand", "args", "[]string", true)
	check("CallU32", "args", "[]uint64", true)
	check("NoVariadic", "xs", "[]string", false)
}

// str-4cqz: function-typed parameters have no JSON representation. The
// wrapper must bake `nil` as a deterministic stub so the generated source
// compiles and runs without trying to json.Unmarshal a JSON value into a
// `func(...)` slot, which produced "param fn: json: cannot unmarshal X
// into Go value of type func(...)" error clusters on every iteration.
func TestGenerateWrapper_FuncParamStubbedAsNil(t *testing.T) {
	const src = `package cb

func ApplyCallback(s string, fn func(string) error) int {
	if fn == nil {
		return len(s)
	}
	if err := fn(s); err != nil {
		return -1
	}
	return 1
}
`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "cb.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Defs:  map[*ast.Ident]types.Object{},
		Uses:  map[*ast.Ident]types.Object{},
		Types: map[ast.Expr]types.TypeAndValue{},
	}
	conf := types.Config{Importer: importer.Default()}
	tpkg, err := conf.Check("cb", fset, []*ast.File{file}, info)
	if err != nil {
		t.Fatalf("type-check: %v", err)
	}
	pkg := &packages.Package{
		Name:      "cb",
		PkgPath:   "example.com/cb",
		Syntax:    []*ast.File{file},
		Types:     tpkg,
		TypesInfo: info,
	}

	targets := BuildWrapperTargets(pkg)
	if len(targets) != 1 {
		t.Fatalf("len(targets) = %d, want 1", len(targets))
	}
	target := targets[0]
	if len(target.Parameters) != 2 {
		t.Fatalf("len(parameters) = %d, want 2", len(target.Parameters))
	}
	// The string parameter still flows through json.Unmarshal.
	if target.Parameters[0].RuntimeValueExpr != "" {
		t.Errorf("string param RuntimeValueExpr = %q, want empty",
			target.Parameters[0].RuntimeValueExpr)
	}
	// The func-typed parameter must be deterministically stubbed with nil.
	if target.Parameters[1].RuntimeValueExpr != "nil" {
		t.Errorf("func param RuntimeValueExpr = %q, want %q",
			target.Parameters[1].RuntimeValueExpr, "nil")
	}

	out := GenerateWrapper("cb", targets, nil)
	if !strings.Contains(out, "func(string) error = nil") {
		t.Errorf("wrapper missing func-param nil assignment; source:\n%s", out)
	}
	if strings.Contains(out, "json.Unmarshal(_shatterInputs[1], &fn)") {
		t.Errorf("wrapper still decodes fn from inputs; func-param stub should bypass json.Unmarshal; source:\n%s", out)
	}
}

// TestIsFuncTypeSpelling covers the shape-based detector used by
// applyRuntimeValueBindings to recognise function-typed parameters whose
// exact Go-source spelling varies per signature.
func TestIsFuncTypeSpelling(t *testing.T) {
	cases := []struct {
		in   string
		want bool
	}{
		{"func()", true},
		{"func(string) error", true},
		{"func(int) (T, error)", true},
		{"  func(int) bool", true},
		{"function", false},
		{"funcation", false},
		{"*func()", false},
		{"", false},
		{"chan func()", false},
		{"map[string]func()", false},
	}
	for _, tc := range cases {
		got := isFuncTypeSpelling(tc.in)
		if got != tc.want {
			t.Errorf("isFuncTypeSpelling(%q) = %v, want %v", tc.in, got, tc.want)
		}
	}
}
