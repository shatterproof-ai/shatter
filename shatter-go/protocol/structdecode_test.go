package protocol

import (
	"go/ast"
	"go/importer"
	"go/parser"
	"go/token"
	"go/types"
	"testing"
)

// parseAndTypeCheckSource type-checks src with a source-mode importer, which
// (unlike parseAndTypeCheck's importer.Default()) can resolve non-stdlib
// module dependencies such as gopkg.in/yaml.v3 from the local module cache —
// needed for tests exercising yaml.Unmarshal detection. Production analysis
// resolves real imports via the golang.org/x/tools/go/packages loader
// (loadPackageForAnalysis), which has no such stdlib-only limitation; this
// helper exists only to make that same resolution available in a
// single-file unit test.
func parseAndTypeCheckSource(t *testing.T, src string) (*ast.File, *types.Info, *token.FileSet) {
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
		Importer: importer.ForCompiler(fset, "source", nil),
		Error:    func(error) {},
	}
	conf.Check(file.Name.Name, fset, []*ast.File{file}, info) //nolint:errcheck
	return file, info, fset
}

func TestStructDecodeSeedsByParamDirectJSONUnmarshal(t *testing.T) {
	src := `package p

import "encoding/json"

type Spec struct {
	OpenAPI string ` + "`json:\"openapi\"`" + `
}

func ClassifySpec(data []byte) string {
	var spec Spec
	if err := json.Unmarshal(data, &spec); err != nil {
		return "invalid"
	}
	return spec.OpenAPI
}
`
	file, info, _ := parseAndTypeCheck(t, src)
	fn := findFuncDecl(file, "ClassifySpec")
	if fn == nil {
		t.Fatal("ClassifySpec not found")
	}
	params := []ParamInfo{{Name: "data", Type: TypeInfo{Kind: "array"}}}
	got := structDecodeSeedsByParam(fn, info, params)
	seeds, ok := got["data"]
	if !ok {
		t.Fatalf("expected seeds for param %q, got %v", "data", got)
	}
	if len(seeds) < 2 {
		t.Fatalf("expected at least 2 seeds (schema doc + empty-object fallback), got %d: %v", len(seeds), seeds)
	}
	if string(seeds[0]) == "{}" {
		t.Errorf("expected the schema-derived document to rank first, got %s", seeds[0])
	}
}

func TestStructDecodeSeedsByParamYAMLUnmarshalLocalAlias(t *testing.T) {
	src := `package p

import "gopkg.in/yaml.v3"

type Config struct {
	MaxRetries int
}

func LoadConfig(raw []byte) (*Config, error) {
	body := raw
	var cfg Config
	if err := yaml.Unmarshal(body, &cfg); err != nil {
		return nil, err
	}
	return &cfg, nil
}
`
	file, info, _ := parseAndTypeCheckSource(t, src)
	fn := findFuncDecl(file, "LoadConfig")
	if fn == nil {
		t.Fatal("LoadConfig not found")
	}
	params := []ParamInfo{{Name: "raw", Type: TypeInfo{Kind: "array"}}}
	got := structDecodeSeedsByParam(fn, info, params)
	if _, ok := got["raw"]; !ok {
		t.Fatalf("expected seeds for aliased param %q via one-level local alias, got %v", "raw", got)
	}
}

func TestStructDecodeSeedsByParamNoDecodeCallReturnsNil(t *testing.T) {
	src := `package p

func Add(a, b int) int {
	return a + b
}
`
	file, info, _ := parseAndTypeCheck(t, src)
	fn := findFuncDecl(file, "Add")
	params := []ParamInfo{
		{Name: "a", Type: TypeInfo{Kind: "int"}},
		{Name: "b", Type: TypeInfo{Kind: "int"}},
	}
	got := structDecodeSeedsByParam(fn, info, params)
	if got != nil {
		t.Errorf("expected nil for a function with no decode call, got %v", got)
	}
}

func TestStructDecodeSeedsByParamFilePathIndirectionNotDetected(t *testing.T) {
	src := `package p

import (
	"encoding/json"
	"os"
)

type Spec struct {
	OpenAPI string
}

func LoadSpec(path string) (*Spec, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var spec Spec
	if err := json.Unmarshal(data, &spec); err != nil {
		return nil, err
	}
	return &spec, nil
}
`
	file, info, _ := parseAndTypeCheck(t, src)
	fn := findFuncDecl(file, "LoadSpec")
	params := []ParamInfo{{Name: "path", Type: TypeInfo{Kind: "str"}}}
	got := structDecodeSeedsByParam(fn, info, params)
	if got != nil {
		t.Errorf("file-path-indirected decode is out of scope for str-4q7bd; expected nil, got %v", got)
	}
}

// str-4q7bd regression: unwrapToIdent must not peel arbitrary single-argument
// function calls, only genuine type conversions ([]byte(x), string(x)).
// Before this fix, decrypt(data) unwrapped to the identifier "data" just like
// a conversion, so structDecodeSeedsByParam attributed the decode site to the
// untransformed parameter and would have seeded it with a plaintext document
// that decrypt() can never turn back into valid JSON — silently useless.
func TestStructDecodeSeedsByParamNonConversionCallNotUnwrapped(t *testing.T) {
	src := `package p

import "encoding/json"

type Spec struct {
	OpenAPI string
}

func decrypt(b []byte) []byte {
	return b
}

func ClassifySpec(data []byte) string {
	var spec Spec
	if err := json.Unmarshal(decrypt(data), &spec); err != nil {
		return "invalid"
	}
	return spec.OpenAPI
}
`
	file, info, _ := parseAndTypeCheck(t, src)
	fn := findFuncDecl(file, "ClassifySpec")
	if fn == nil {
		t.Fatal("ClassifySpec not found")
	}
	params := []ParamInfo{{Name: "data", Type: TypeInfo{Kind: "array"}}}
	got := structDecodeSeedsByParam(fn, info, params)
	if got != nil {
		t.Errorf("decrypt(data) is a function call, not a type conversion; must not resolve to param %q, got %v", "data", got)
	}
}
