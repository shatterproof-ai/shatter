package protocol

import (
	"encoding/json"
	"go/types"
	"testing"

	yaml "gopkg.in/yaml.v3"
)

// findNamedStructType type-checks src and returns the *types.Struct
// underlying the named type typeName declared at package scope.
func findNamedStructType(t *testing.T, src string, typeName string) types.Type {
	t.Helper()
	file, info, _ := parseAndTypeCheck(t, src)
	_ = file
	for ident, obj := range info.Defs {
		tn, ok := obj.(*types.TypeName)
		if !ok || ident.Name != typeName {
			continue
		}
		return tn.Type()
	}
	t.Fatalf("type %s not found", typeName)
	return nil
}

func TestSynthesizeStructDocumentFlatPrimitives(t *testing.T) {
	src := `package p

type Config struct {
	Name    string
	Enabled bool
	Count   int
	Ratio   float64
}
`
	typ := findNamedStructType(t, src, "Config")
	doc, ok := synthesizeStructDocument(typ, "json")
	if !ok {
		t.Fatal("expected ok=true")
	}
	raw, err := json.Marshal(doc)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var cfg struct {
		Name    string
		Enabled bool
		Count   int
		Ratio   float64
	}
	if err := json.Unmarshal(raw, &cfg); err != nil {
		t.Fatalf("round-trip unmarshal: %v", err)
	}
	if cfg.Name == "" || !cfg.Enabled || cfg.Count == 0 || cfg.Ratio == 0 {
		t.Errorf("expected non-zero synthesized fields, got %+v", cfg)
	}
}

func TestSynthesizeStructDocumentHonorsJSONTags(t *testing.T) {
	src := `package p

type Spec struct {
	OpenAPI string ` + "`json:\"openapi\"`" + `
	Ignored string ` + "`json:\"-\"`" + `
	Info    struct {
		Title string ` + "`json:\"title\"`" + `
	} ` + "`json:\"info\"`" + `
}
`
	typ := findNamedStructType(t, src, "Spec")
	doc, ok := synthesizeStructDocument(typ, "json")
	if !ok {
		t.Fatal("expected ok=true")
	}
	if _, has := doc["OpenAPI"]; has {
		t.Errorf("expected tag-resolved key %q, not raw field name; doc=%v", "OpenAPI", doc)
	}
	if _, has := doc["openapi"]; !has {
		t.Errorf("expected tag-resolved key %q present; doc=%v", "openapi", doc)
	}
	if _, has := doc["Ignored"]; has {
		t.Errorf("json:\"-\" field must be omitted; doc=%v", doc)
	}
	info, ok := doc["info"].(map[string]any)
	if !ok {
		t.Fatalf("expected nested info object, got %T: %v", doc["info"], doc["info"])
	}
	if _, has := info["title"]; !has {
		t.Errorf("expected nested tag-resolved key %q; info=%v", "title", info)
	}
}

func TestSynthesizeStructDocumentYAMLDefaultLowercase(t *testing.T) {
	src := `package p

type Config struct {
	MaxRetries int
}
`
	typ := findNamedStructType(t, src, "Config")
	doc, ok := synthesizeStructDocument(typ, "yaml")
	if !ok {
		t.Fatal("expected ok=true")
	}
	if _, has := doc["maxretries"]; !has {
		t.Errorf("expected yaml.v3 default lowercase key %q; doc=%v", "maxretries", doc)
	}
	raw, err := yaml.Marshal(doc)
	if err != nil {
		t.Fatalf("yaml marshal: %v", err)
	}
	var cfg struct {
		MaxRetries int
	}
	if err := yaml.Unmarshal(raw, &cfg); err != nil {
		t.Fatalf("yaml round-trip unmarshal: %v", err)
	}
	if cfg.MaxRetries == 0 {
		t.Errorf("expected non-zero MaxRetries, got %+v", cfg)
	}
}

func TestSynthesizeStructDocumentNestedSliceAndMap(t *testing.T) {
	src := `package p

type Item struct {
	Name string ` + "`json:\"name\"`" + `
}

type Doc struct {
	Items []Item            ` + "`json:\"items\"`" + `
	Attrs map[string]string ` + "`json:\"attrs\"`" + `
}
`
	typ := findNamedStructType(t, src, "Doc")
	doc, ok := synthesizeStructDocument(typ, "json")
	if !ok {
		t.Fatal("expected ok=true")
	}
	raw, err := json.Marshal(doc)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var out struct {
		Items []struct {
			Name string `json:"name"`
		} `json:"items"`
		Attrs map[string]string `json:"attrs"`
	}
	if err := json.Unmarshal(raw, &out); err != nil {
		t.Fatalf("round-trip unmarshal: %v", err)
	}
	if len(out.Items) == 0 || out.Items[0].Name == "" {
		t.Errorf("expected populated Items slice, got %+v", out.Items)
	}
	if len(out.Attrs) == 0 {
		t.Errorf("expected populated Attrs map, got %+v", out.Attrs)
	}
}

func TestSynthesizeStructDocumentSelfReferentialDoesNotHang(t *testing.T) {
	src := `package p

type Node struct {
	Name     string  ` + "`json:\"name\"`" + `
	Children []*Node ` + "`json:\"children\"`" + `
}
`
	typ := findNamedStructType(t, src, "Node")
	doc, ok := synthesizeStructDocument(typ, "json")
	if !ok {
		t.Fatal("expected ok=true")
	}
	raw, err := json.Marshal(doc)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	type node struct {
		Name     string  `json:"name"`
		Children []*node `json:"children"`
	}
	var out node
	if err := json.Unmarshal(raw, &out); err != nil {
		t.Fatalf("round-trip unmarshal: %v", err)
	}
}

func TestSynthesizeStructDocumentNonStructReturnsFalse(t *testing.T) {
	src := `package p

type Alias = string
`
	typ := findNamedStructType(t, src, "Alias")
	if _, ok := synthesizeStructDocument(typ, "json"); ok {
		t.Fatal("expected ok=false for non-struct type")
	}
}
