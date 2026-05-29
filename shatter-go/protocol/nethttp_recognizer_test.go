package protocol

import (
	"go/ast"
	"go/importer"
	"go/parser"
	"go/token"
	"go/types"
	"testing"
)

// parseAndTypeCheck parses Go source and returns the AST file, type info, and fset.
func parseAndTypeCheck(t *testing.T, src string) (*ast.File, *types.Info, *token.FileSet) {
	t.Helper()
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "test.go", src, 0)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	info := &types.Info{
		Types: make(map[ast.Expr]types.TypeAndValue),
		Defs:  make(map[*ast.Ident]types.Object),
		Uses:  make(map[*ast.Ident]types.Object),
	}
	conf := types.Config{
		Importer: importer.Default(),
		Error:    func(error) {},
	}
	conf.Check(file.Name.Name, fset, []*ast.File{file}, info) //nolint:errcheck
	return file, info, fset
}

// findFuncDecl finds a FuncDecl by name in the AST.
func findFuncDecl(file *ast.File, name string) *ast.FuncDecl {
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if ok && fn.Name.Name == name {
			return fn
		}
	}
	return nil
}

func TestRecognizeHTTPHandler_StandardHandler(t *testing.T) {
	src := `package main

import "net/http"

func Handler(w http.ResponseWriter, r *http.Request) {}
`
	file, info, _ := parseAndTypeCheck(t, src)
	fn := findFuncDecl(file, "Handler")
	if fn == nil {
		t.Fatal("function Handler not found")
	}
	model := recognizeHTTPHandler(fn, info)
	if model == nil {
		t.Fatal("expected InvocationModel, got nil")
	}
	if model.Kind != "adapter" {
		t.Fatalf("expected kind adapter, got %s", model.Kind)
	}
	if model.AdapterID != HTTPHandlerAdapterID {
		t.Fatalf("expected adapter_id %s, got %s", HTTPHandlerAdapterID, model.AdapterID)
	}
	if len(model.SyntheticParams) != 4 {
		t.Fatalf("expected 4 synthetic params, got %d", len(model.SyntheticParams))
	}
	expectedNames := []string{"method", "path", "headers", "body"}
	for i, name := range expectedNames {
		if model.SyntheticParams[i].Name != name {
			t.Errorf("synthetic param %d: expected %s, got %s", i, name, model.SyntheticParams[i].Name)
		}
	}
}

func TestRecognizeHTTPHandler_NonHandler(t *testing.T) {
	src := `package main

func Add(a, b int) int { return a + b }
`
	file, info, _ := parseAndTypeCheck(t, src)
	fn := findFuncDecl(file, "Add")
	if fn == nil {
		t.Fatal("function Add not found")
	}
	model := recognizeHTTPHandler(fn, info)
	if model != nil {
		t.Fatalf("expected nil for non-handler, got %+v", model)
	}
}

func TestRecognizeHTTPHandler_PartialMatch(t *testing.T) {
	src := `package main

import "net/http"

func Partial(w http.ResponseWriter, n int) {}
`
	file, info, _ := parseAndTypeCheck(t, src)
	fn := findFuncDecl(file, "Partial")
	if fn == nil {
		t.Fatal("function Partial not found")
	}
	model := recognizeHTTPHandler(fn, info)
	if model != nil {
		t.Fatalf("expected nil for partial match, got %+v", model)
	}
}

func TestRecognizeHTTPHandler_MethodReceiverDoesNotUseAdapterInvocation(t *testing.T) {
	src := `package main

import "net/http"

type Server struct{}

func (s *Server) Handle(w http.ResponseWriter, r *http.Request) {}
`
	file, info, _ := parseAndTypeCheck(t, src)
	fn := findFuncDecl(file, "Handle")
	if fn == nil {
		t.Fatal("function Handle not found")
	}
	model := recognizeHTTPHandler(fn, info)
	if model != nil {
		t.Fatalf("expected nil InvocationModel for method handler, got %+v", model)
	}
}

func TestRecognizeHTTPHandler_UnnamedParams(t *testing.T) {
	src := `package main

import "net/http"

func Unnamed(http.ResponseWriter, *http.Request) {}
`
	file, info, _ := parseAndTypeCheck(t, src)
	fn := findFuncDecl(file, "Unnamed")
	if fn == nil {
		t.Fatal("function Unnamed not found")
	}
	model := recognizeHTTPHandler(fn, info)
	if model == nil {
		t.Fatal("expected InvocationModel for unnamed params, got nil")
	}
	if model.AdapterID != HTTPHandlerAdapterID {
		t.Fatalf("expected adapter_id %s, got %s", HTTPHandlerAdapterID, model.AdapterID)
	}
}

func TestRecognizeHTTPHandler_NoParams(t *testing.T) {
	src := `package main

func NoParams() {}
`
	file, info, _ := parseAndTypeCheck(t, src)
	fn := findFuncDecl(file, "NoParams")
	if fn == nil {
		t.Fatal("function NoParams not found")
	}
	model := recognizeHTTPHandler(fn, info)
	if model != nil {
		t.Fatalf("expected nil for no-params function, got %+v", model)
	}
}

func TestRecognizeHTTPHandler_ReversedParamOrder(t *testing.T) {
	src := `package main

import "net/http"

func Reversed(r *http.Request, w http.ResponseWriter) {}
`
	file, info, _ := parseAndTypeCheck(t, src)
	fn := findFuncDecl(file, "Reversed")
	if fn == nil {
		t.Fatal("function Reversed not found")
	}
	model := recognizeHTTPHandler(fn, info)
	if model != nil {
		t.Fatalf("expected nil for reversed param order, got %+v", model)
	}
}
