package protocol

import (
	"encoding/json"
	"fmt"
	"go/types"
	"reflect"
	"strings"
	"testing"

	yaml "gopkg.in/yaml.v3"
	"pgregory.net/rapid"
)

// primitiveFieldKind describes one property-generated struct field: its Go
// source type, its reflect.Type twin (used to build a real destination
// struct via reflect.StructOf so the property exercises an actual
// encoding/json or gopkg.in/yaml.v3 decode, not just "is this valid JSON"),
// and a predicate checking the decoded value is the expected non-zero
// synthesized representative rather than a zero value.
type primitiveFieldKind struct {
	goSrcType   string
	reflectType reflect.Type
	isZero      func(v reflect.Value) bool
}

var primitiveFieldKinds = []primitiveFieldKind{
	{"string", reflect.TypeOf(""), func(v reflect.Value) bool { return v.String() == "" }},
	{"int", reflect.TypeOf(int(0)), func(v reflect.Value) bool { return v.Int() == 0 }},
	{"bool", reflect.TypeOf(false), func(v reflect.Value) bool { return !v.Bool() }},
	{"float64", reflect.TypeOf(float64(0)), func(v reflect.Value) bool { return v.Float() == 0 }},
}

// TestSynthesizeStructDocumentRoundTripsProperty is the core str-4q7bd
// invariant from the acceptance criteria: for any struct made of resolvable
// primitive fields, synthesizeStructDocument's output is a structurally
// valid document that a REAL json.Unmarshal/yaml.Unmarshal into the target
// struct type accepts and populates with non-zero values (not just
// "produces syntactically valid JSON/YAML" — the destination struct is
// built dynamically via reflect.StructOf from the same field spec used to
// type-check the source struct, so the test exercises an actual decode into
// the modeled shape).
func TestSynthesizeStructDocumentRoundTripsProperty(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		format := rapid.SampledFrom([]string{"json", "yaml"}).Draw(rt, "format")
		fieldCount := rapid.IntRange(0, 6).Draw(rt, "fieldCount")

		var srcFields []string
		var reflectFields []reflect.StructField
		var checks []func(decoded reflect.Value) error

		for i := 0; i < fieldCount; i++ {
			kind := rapid.SampledFrom(primitiveFieldKinds).Draw(rt, fmt.Sprintf("kind%d", i))
			fieldName := fmt.Sprintf("F%d", i)
			wireName := fmt.Sprintf("f%d", i)
			tagKey := "json"
			if format == "yaml" {
				tagKey = "yaml"
			}
			srcFields = append(srcFields, fmt.Sprintf("\t%s %s `%s:\"%s\"`\n", fieldName, kind.goSrcType, tagKey, wireName))
			reflectFields = append(reflectFields, reflect.StructField{
				Name: fieldName,
				Type: kind.reflectType,
				Tag:  reflect.StructTag(fmt.Sprintf(`%s:"%s"`, tagKey, wireName)),
			})
			idx, k, name := i, kind, fieldName
			checks = append(checks, func(decoded reflect.Value) error {
				fv := decoded.Field(idx)
				if k.isZero(fv) {
					return fmt.Errorf("field %s decoded as zero value: %v (expected the synthesizer's non-zero representative)", name, fv.Interface())
				}
				return nil
			})
		}

		src := "package p\n\ntype S struct {\n" + strings.Join(srcFields, "") + "}\n"
		typ := parsePropertyStructType(t, src, "S")

		doc, ok := synthesizeStructDocument(typ, format)
		if !ok {
			rt.Fatal("expected ok=true for an all-primitive-field struct")
		}

		dstType := reflect.StructOf(reflectFields)
		dstPtr := reflect.New(dstType)

		if format == "json" {
			raw, err := json.Marshal(doc)
			if err != nil {
				rt.Fatalf("json.Marshal(doc): %v", err)
			}
			if err := json.Unmarshal(raw, dstPtr.Interface()); err != nil {
				rt.Fatalf("json.Unmarshal into real destination struct failed: %v; doc=%v", err, doc)
			}
		} else {
			raw, err := yaml.Marshal(doc)
			if err != nil {
				rt.Fatalf("yaml.Marshal(doc): %v", err)
			}
			if err := yaml.Unmarshal(raw, dstPtr.Interface()); err != nil {
				rt.Fatalf("yaml.Unmarshal into real destination struct failed: %v; doc=%v", err, doc)
			}
		}

		decoded := dstPtr.Elem()
		for _, check := range checks {
			if err := check(decoded); err != nil {
				rt.Fatal(err)
			}
		}
	})
}

// parsePropertyStructType type-checks src and returns the go/types.Type of
// the package-scope type declaration named typeName.
func parsePropertyStructType(t *testing.T, src string, typeName string) types.Type {
	t.Helper()
	_, info, _ := parseAndTypeCheck(t, src)
	for ident, obj := range info.Defs {
		if tn, ok := obj.(*types.TypeName); ok && ident.Name == typeName {
			return tn.Type()
		}
	}
	t.Fatalf("type %s not found in generated source:\n%s", typeName, src)
	return nil
}
