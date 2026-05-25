package protocol

import (
	"go/ast"
	"go/importer"
	"go/parser"
	"go/token"
	"go/types"
	"path/filepath"
	"strings"
	"testing"
)

// parseTestFile parses a testdata Go file and returns the AST, FileSet, and type info.
func parseTestFile(t *testing.T, filename string) (*token.FileSet, *ast.File, *types.Info) {
	t.Helper()
	fset := token.NewFileSet()
	path := filepath.Join("testdata", filename)
	file, err := parser.ParseFile(fset, path, nil, parser.ParseComments)
	if err != nil {
		t.Fatalf("parse %s: %v", filename, err)
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
	return fset, file, info
}

// analyzeTestFile runs the full analysis pipeline on a testdata file.
func analyzeTestFile(t *testing.T, filename string) []FunctionAnalysis {
	t.Helper()
	path := filepath.Join("testdata", filename)
	results, err := AnalyzeFile(path, "")
	if err != nil {
		t.Fatalf("AnalyzeFile(%s): %v", filename, err)
	}
	return results
}

func findAnalysis(results []FunctionAnalysis, name string) *FunctionAnalysis {
	for i := range results {
		if results[i].Name == name {
			return &results[i]
		}
	}
	return nil
}

// --- net/http recognizer tests ---

func TestRecognizeNetHTTP_StandardHandler(t *testing.T) {
	fset, file, info := parseTestFile(t, "http_handler.go")
	functions := analyzeTestFile(t, "http_handler.go")

	hints := RecognizeNetHTTPHandlers(fset, file, info, functions)

	if len(hints) != len(functions) {
		t.Fatalf("hints length %d != functions length %d", len(hints), len(functions))
	}

	idx := -1
	for i, f := range functions {
		if f.Name == "HandleRequest" {
			idx = i
			break
		}
	}
	if idx == -1 {
		t.Fatal("HandleRequest not found in analysis")
	}
	hint := hints[idx]
	if hint == nil {
		t.Fatal("expected hint for HandleRequest, got nil")
	}
	if hint.Adapter.ID != HTTPHandlerAdapterID {
		t.Errorf("adapter ID = %q, want %q", hint.Adapter.ID, HTTPHandlerAdapterID)
	}
	if hint.Confidence != "high" {
		t.Errorf("confidence = %q, want %q", hint.Confidence, "high")
	}
	if len(hint.Reasons) < 2 {
		t.Errorf("expected at least 2 reasons, got %d: %v", len(hint.Reasons), hint.Reasons)
	}
}

func TestRecognizeNetHTTP_ServeHTTPMethod(t *testing.T) {
	fset, file, info := parseTestFile(t, "http_handler.go")
	functions := analyzeTestFile(t, "http_handler.go")

	hints := RecognizeNetHTTPHandlers(fset, file, info, functions)

	// str-fuhw.1.1: methods now surface with receiver-decorated names
	// (e.g. "(*myHandler).ServeHTTP"). Match by suffix so the recognizer
	// fixture stays receiver-agnostic.
	idx := -1
	for i, f := range functions {
		if f.Name == "ServeHTTP" || strings.HasSuffix(f.Name, ".ServeHTTP") {
			idx = i
			break
		}
	}
	if idx == -1 {
		t.Fatal("ServeHTTP not found")
	}
	hint := hints[idx]
	if hint == nil {
		t.Fatal("expected hint for ServeHTTP, got nil")
	}
	if hint.Confidence != "high" {
		t.Errorf("confidence = %q, want %q", hint.Confidence, "high")
	}
	hasServeHTTPReason := false
	for _, r := range hint.Reasons {
		if r == "Method ServeHTTP implements net/http.Handler interface" {
			hasServeHTTPReason = true
		}
	}
	if !hasServeHTTPReason {
		t.Errorf("expected ServeHTTP reason, got: %v", hint.Reasons)
	}
}

func TestRecognizeNetHTTP_PartialMatch(t *testing.T) {
	fset, file, info := parseTestFile(t, "http_handler.go")
	functions := analyzeTestFile(t, "http_handler.go")

	hints := RecognizeNetHTTPHandlers(fset, file, info, functions)

	idx := -1
	for i, f := range functions {
		if f.Name == "WriteResponse" {
			idx = i
			break
		}
	}
	if idx == -1 {
		t.Fatal("WriteResponse not found")
	}
	hint := hints[idx]
	if hint == nil {
		t.Fatal("expected hint for WriteResponse, got nil")
	}
	if hint.Confidence != "medium" {
		t.Errorf("confidence = %q, want %q", hint.Confidence, "medium")
	}
}

func TestRecognizeNetHTTP_NoMatch(t *testing.T) {
	fset, file, info := parseTestFile(t, "http_handler.go")
	functions := analyzeTestFile(t, "http_handler.go")

	hints := RecognizeNetHTTPHandlers(fset, file, info, functions)

	for i, f := range functions {
		if f.Name == "HelperFunc" {
			if hints[i] != nil {
				t.Errorf("expected nil hint for HelperFunc, got %+v", hints[i])
			}
		}
	}
}

func TestRecognizeNetHTTP_NoImport(t *testing.T) {
	fset, file, info := parseTestFile(t, "basic.go")
	functions := analyzeTestFile(t, "basic.go")

	hints := RecognizeNetHTTPHandlers(fset, file, info, functions)

	for i, hint := range hints {
		if hint != nil {
			t.Errorf("function %d: expected nil hint without net/http import, got %+v", i, hint)
		}
	}
}

// --- Gin recognizer tests ---

func TestRecognizeGin_StandardHandler(t *testing.T) {
	fset, file, info := parseTestFile(t, "gin_handler.go")
	functions := analyzeTestFile(t, "gin_handler.go")

	hints := RecognizeGinHandlers(fset, file, info, functions)

	if len(hints) != len(functions) {
		t.Fatalf("hints length %d != functions length %d", len(hints), len(functions))
	}

	idx := -1
	for i, f := range functions {
		if f.Name == "ListUsers" {
			idx = i
			break
		}
	}
	if idx == -1 {
		t.Fatal("ListUsers not found")
	}
	hint := hints[idx]
	if hint == nil {
		t.Fatal("expected hint for ListUsers, got nil")
	}
	if hint.Adapter.ID != GinAdapterID {
		t.Errorf("adapter ID = %q, want %q", hint.Adapter.ID, GinAdapterID)
	}
	if hint.Confidence != "high" {
		t.Errorf("confidence = %q, want %q", hint.Confidence, "high")
	}
}

func TestRecognizeGin_WithAPIUsage(t *testing.T) {
	fset, file, info := parseTestFile(t, "gin_handler.go")
	functions := analyzeTestFile(t, "gin_handler.go")

	hints := RecognizeGinHandlers(fset, file, info, functions)

	idx := -1
	for i, f := range functions {
		if f.Name == "CreateUser" {
			idx = i
			break
		}
	}
	if idx == -1 {
		t.Fatal("CreateUser not found")
	}
	hint := hints[idx]
	if hint == nil {
		t.Fatal("expected hint for CreateUser, got nil")
	}
	if len(hint.Reasons) < 2 {
		t.Errorf("expected at least 2 reasons (param + API calls), got %d: %v", len(hint.Reasons), hint.Reasons)
	}
}

func TestRecognizeGin_NoMatch(t *testing.T) {
	fset, file, info := parseTestFile(t, "gin_handler.go")
	functions := analyzeTestFile(t, "gin_handler.go")

	hints := RecognizeGinHandlers(fset, file, info, functions)

	for i, f := range functions {
		if f.Name == "GinHelper" {
			if hints[i] != nil {
				t.Errorf("expected nil hint for GinHelper, got %+v", hints[i])
			}
		}
	}
}

func TestRecognizeGin_NoImport(t *testing.T) {
	fset, file, info := parseTestFile(t, "basic.go")
	functions := analyzeTestFile(t, "basic.go")

	hints := RecognizeGinHandlers(fset, file, info, functions)

	for i, hint := range hints {
		if hint != nil {
			t.Errorf("function %d: expected nil hint without gin import, got %+v", i, hint)
		}
	}
}

// --- Import alias map tests ---

func TestBuildImportAliasMap(t *testing.T) {
	fset := token.NewFileSet()
	src := `package test
import (
	"fmt"
	"net/http"
	mygin "github.com/gin-gonic/gin"
)
func F() {}
`
	file, err := parser.ParseFile(fset, "test.go", src, 0)
	if err != nil {
		t.Fatal(err)
	}
	m := buildImportAliasMap(file)

	tests := []struct {
		importPath string
		wantAlias  string
	}{
		{"fmt", "fmt"},
		{"net/http", "http"},
		{"github.com/gin-gonic/gin", "mygin"},
	}
	for _, tt := range tests {
		got, ok := m[tt.importPath]
		if !ok {
			t.Errorf("import %q not found in map", tt.importPath)
			continue
		}
		if got != tt.wantAlias {
			t.Errorf("alias for %q = %q, want %q", tt.importPath, got, tt.wantAlias)
		}
	}
}

// --- Integration tests via AnalyzeFile ---

func TestAnalyzeFileHTTPHandler(t *testing.T) {
	results := analyzeTestFile(t, "http_handler.go")

	handler := findAnalysis(results, "HandleRequest")
	if handler == nil {
		t.Fatal("HandleRequest not found")
	}
	if len(handler.AdapterHints) == 0 {
		t.Fatal("expected adapter_hints on HandleRequest")
	}
	if handler.AdapterHints[0].Adapter.ID != HTTPHandlerAdapterID {
		t.Errorf("adapter ID = %q, want %q", handler.AdapterHints[0].Adapter.ID, HTTPHandlerAdapterID)
	}
	if handler.AdapterHints[0].Confidence != "high" {
		t.Errorf("confidence = %q, want %q", handler.AdapterHints[0].Confidence, "high")
	}
	// InvocationModel should be set by recognizeHTTPHandler (from nethttp_recognizer.go).
	if handler.InvocationModel == nil {
		t.Fatal("expected invocation_model on HandleRequest")
	}
	if handler.InvocationModel.Kind != "adapter" {
		t.Errorf("invocation_model.kind = %q, want %q", handler.InvocationModel.Kind, "adapter")
	}
	if handler.InvocationModel.AdapterID != HTTPHandlerAdapterID {
		t.Errorf("invocation_model.adapter_id = %q, want %q", handler.InvocationModel.AdapterID, HTTPHandlerAdapterID)
	}

	// HelperFunc should have no hints.
	helper := findAnalysis(results, "HelperFunc")
	if helper == nil {
		t.Fatal("HelperFunc not found")
	}
	if len(helper.AdapterHints) > 0 {
		t.Errorf("expected no adapter_hints on HelperFunc, got %v", helper.AdapterHints)
	}
}

// str-n10u: verify that partial HTTP helpers emit the expected param type
// so the Rust core's executability check can distinguish them from exact handlers.
func TestAnalyzeFileHTTPHandler_PartialHelperParamType(t *testing.T) {
	results := analyzeTestFile(t, "http_handler.go")

	// Exported partial helper.
	wr := findAnalysis(results, "WriteResponse")
	if wr == nil {
		t.Fatal("WriteResponse not found")
	}
	if len(wr.Params) != 1 {
		t.Fatalf("expected 1 param on WriteResponse, got %d", len(wr.Params))
	}
	// http.ResponseWriter is in synthesizableStdlibTypes → Kind: "unknown".
	if wr.Params[0].Type.Kind != "unknown" {
		t.Errorf("WriteResponse param kind = %q, want %q", wr.Params[0].Type.Kind, "unknown")
	}
	// TypeName must be set for planner lookup.
	if wr.Params[0].TypeName == nil || *wr.Params[0].TypeName != "http.ResponseWriter" {
		t.Errorf("WriteResponse param type_name = %v, want %q", wr.Params[0].TypeName, "http.ResponseWriter")
	}

	// Private partial helper — same type classification.
	wp := findAnalysis(results, "writePartial")
	if wp == nil {
		t.Fatal("writePartial not found")
	}
	if len(wp.Params) != 1 {
		t.Fatalf("expected 1 param on writePartial, got %d", len(wp.Params))
	}
	if wp.Params[0].Type.Kind != "unknown" {
		t.Errorf("writePartial param kind = %q, want %q", wp.Params[0].Type.Kind, "unknown")
	}
}

// str-n10u: Private exact HTTP handlers must get the same adapter recognition
// as exported handlers. The exportedness gate is the CLI's concern (--all),
// not the frontend's.
func TestAnalyzeFileHTTPHandler_PrivateExactHandler(t *testing.T) {
	results := analyzeTestFile(t, "http_handler.go")

	priv := findAnalysis(results, "handlePrivate")
	if priv == nil {
		t.Fatal("handlePrivate not found in analysis results")
	}
	if priv.Exported {
		t.Error("handlePrivate should not be marked as exported")
	}
	// Exact private handler: must have InvocationModel set.
	if priv.InvocationModel == nil {
		t.Fatal("expected invocation_model on private exact HTTP handler")
	}
	if priv.InvocationModel.Kind != "adapter" {
		t.Errorf("invocation_model.kind = %q, want %q", priv.InvocationModel.Kind, "adapter")
	}
	if priv.InvocationModel.AdapterID != HTTPHandlerAdapterID {
		t.Errorf("invocation_model.adapter_id = %q, want %q", priv.InvocationModel.AdapterID, HTTPHandlerAdapterID)
	}
	// Must also have adapter hints.
	if len(priv.AdapterHints) == 0 {
		t.Fatal("expected adapter_hints on handlePrivate")
	}
	if priv.AdapterHints[0].Confidence != "high" {
		t.Errorf("confidence = %q, want %q", priv.AdapterHints[0].Confidence, "high")
	}

	// Partial private helper: only ResponseWriter, no *Request.
	partial := findAnalysis(results, "writePartial")
	if partial == nil {
		t.Fatal("writePartial not found in analysis results")
	}
	if partial.Exported {
		t.Error("writePartial should not be marked as exported")
	}
	// Partial match: should have medium-confidence hint but NOT a full InvocationModel.
	if partial.InvocationModel != nil {
		t.Errorf("writePartial should not have InvocationModel (partial signature), got %+v", partial.InvocationModel)
	}
	if len(partial.AdapterHints) == 0 {
		t.Fatal("expected medium-confidence adapter_hint on writePartial (partial match)")
	}
	if partial.AdapterHints[0].Confidence != "medium" {
		t.Errorf("confidence = %q, want %q", partial.AdapterHints[0].Confidence, "medium")
	}
}

func TestAnalyzeFileGinHandler(t *testing.T) {
	results := analyzeTestFile(t, "gin_handler.go")

	handler := findAnalysis(results, "ListUsers")
	if handler == nil {
		t.Fatal("ListUsers not found")
	}
	if len(handler.AdapterHints) == 0 {
		t.Fatal("expected adapter_hints on ListUsers")
	}
	if handler.AdapterHints[0].Adapter.ID != GinAdapterID {
		t.Errorf("adapter ID = %q, want %q", handler.AdapterHints[0].Adapter.ID, GinAdapterID)
	}
	if handler.AdapterHints[0].Confidence != "high" {
		t.Errorf("confidence = %q, want %q", handler.AdapterHints[0].Confidence, "high")
	}
	// High-confidence Gin hint should promote to invocation model.
	if handler.InvocationModel == nil {
		t.Fatal("expected invocation_model promotion for high-confidence Gin hint")
	}
	if handler.InvocationModel.AdapterID != GinAdapterID {
		t.Errorf("invocation_model.adapter_id = %q, want %q", handler.InvocationModel.AdapterID, GinAdapterID)
	}

	// GinHelper should have no hints.
	helper := findAnalysis(results, "GinHelper")
	if helper == nil {
		t.Fatal("GinHelper not found")
	}
	if len(helper.AdapterHints) > 0 {
		t.Errorf("expected no adapter_hints on GinHelper, got %v", helper.AdapterHints)
	}
}
