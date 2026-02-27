package protocol

import (
	"encoding/json"
	"testing"
)

func TestObjectFieldMarshalJSON(t *testing.T) {
	field := ObjectField{Name: "age", Type: TypeInfo{Kind: "int"}}
	data, err := json.Marshal(field)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	want := `["age",{"kind":"int"}]`
	if string(data) != want {
		t.Errorf("got %s, want %s", data, want)
	}
}

func TestObjectFieldUnmarshalJSON(t *testing.T) {
	input := `["name",{"kind":"str"}]`
	var field ObjectField
	if err := json.Unmarshal([]byte(input), &field); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if field.Name != "name" {
		t.Errorf("name = %q, want %q", field.Name, "name")
	}
	if field.Type.Kind != "str" {
		t.Errorf("type.kind = %q, want %q", field.Type.Kind, "str")
	}
}

func TestObjectFieldRoundTrip(t *testing.T) {
	original := ObjectField{
		Name: "items",
		Type: TypeInfo{
			Kind:    "array",
			Element: &TypeInfo{Kind: "int"},
		},
	}
	data, err := json.Marshal(original)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var decoded ObjectField
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if decoded.Name != original.Name {
		t.Errorf("name = %q, want %q", decoded.Name, original.Name)
	}
	if decoded.Type.Kind != "array" {
		t.Errorf("type.kind = %q, want %q", decoded.Type.Kind, "array")
	}
	if decoded.Type.Element == nil || decoded.Type.Element.Kind != "int" {
		t.Errorf("type.element.kind = %v, want int", decoded.Type.Element)
	}
}

func TestTypeInfoRoundTripScalar(t *testing.T) {
	tests := []struct {
		name string
		ti   TypeInfo
		json string
	}{
		{"int", TypeInfo{Kind: "int"}, `{"kind":"int"}`},
		{"float", TypeInfo{Kind: "float"}, `{"kind":"float"}`},
		{"str", TypeInfo{Kind: "str"}, `{"kind":"str"}`},
		{"bool", TypeInfo{Kind: "bool"}, `{"kind":"bool"}`},
		{"unknown", TypeInfo{Kind: "unknown"}, `{"kind":"unknown"}`},
		{"opaque", TypeInfo{Kind: "opaque", Label: "chan int"}, `{"kind":"opaque","label":"chan int"}`},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			data, err := json.Marshal(tt.ti)
			if err != nil {
				t.Fatalf("marshal: %v", err)
			}
			if string(data) != tt.json {
				t.Errorf("got %s, want %s", data, tt.json)
			}
			var decoded TypeInfo
			if err := json.Unmarshal(data, &decoded); err != nil {
				t.Fatalf("unmarshal: %v", err)
			}
			if decoded.Kind != tt.ti.Kind {
				t.Errorf("kind = %q, want %q", decoded.Kind, tt.ti.Kind)
			}
			if decoded.Label != tt.ti.Label {
				t.Errorf("label = %q, want %q", decoded.Label, tt.ti.Label)
			}
		})
	}
}

func TestTypeInfoRoundTripOpaque(t *testing.T) {
	ti := TypeInfo{Kind: "opaque", Label: "net.Conn"}
	data, err := json.Marshal(ti)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	want := `{"kind":"opaque","label":"net.Conn"}`
	if string(data) != want {
		t.Errorf("got %s, want %s", data, want)
	}
	var decoded TypeInfo
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if decoded.Kind != "opaque" {
		t.Errorf("kind = %q, want opaque", decoded.Kind)
	}
	if decoded.Label != "net.Conn" {
		t.Errorf("label = %q, want net.Conn", decoded.Label)
	}
}

func TestTypeInfoOpaqueOmitsLabelWhenEmpty(t *testing.T) {
	ti := TypeInfo{Kind: "int"}
	data, err := json.Marshal(ti)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	got := string(data)
	if got != `{"kind":"int"}` {
		t.Errorf("got %s, want no label field", got)
	}
}

func TestTypeInfoRoundTripObject(t *testing.T) {
	ti := TypeInfo{
		Kind: "object",
		Fields: []ObjectField{
			{Name: "id", Type: TypeInfo{Kind: "int"}},
			{Name: "name", Type: TypeInfo{Kind: "str"}},
		},
	}
	data, err := json.Marshal(ti)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}

	want := `{"kind":"object","fields":[["id",{"kind":"int"}],["name",{"kind":"str"}]]}`
	if string(data) != want {
		t.Errorf("got %s, want %s", data, want)
	}

	var decoded TypeInfo
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if len(decoded.Fields) != 2 {
		t.Fatalf("fields len = %d, want 2", len(decoded.Fields))
	}
	if decoded.Fields[0].Name != "id" || decoded.Fields[0].Type.Kind != "int" {
		t.Errorf("fields[0] = %+v, want {id, int}", decoded.Fields[0])
	}
}

func TestTypeInfoRoundTripNullable(t *testing.T) {
	ti := TypeInfo{
		Kind:  "nullable",
		Inner: &TypeInfo{Kind: "str"},
	}
	data, err := json.Marshal(ti)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var decoded TypeInfo
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if decoded.Inner == nil || decoded.Inner.Kind != "str" {
		t.Errorf("inner = %v, want str", decoded.Inner)
	}
}

func TestRequestDeserializeHandshake(t *testing.T) {
	input := `{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze","execute"]}`
	var req Request
	if err := json.Unmarshal([]byte(input), &req); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if req.Command != "handshake" {
		t.Errorf("command = %q, want handshake", req.Command)
	}
	if len(req.Capabilities) != 2 {
		t.Errorf("capabilities len = %d, want 2", len(req.Capabilities))
	}
}

func TestRequestDeserializeAnalyze(t *testing.T) {
	input := `{"protocol_version":"0.1.0","id":2,"command":"analyze","file":"src/main.go","function":"Foo"}`
	var req Request
	if err := json.Unmarshal([]byte(input), &req); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if req.Command != "analyze" {
		t.Errorf("command = %q, want analyze", req.Command)
	}
	if req.File != "src/main.go" {
		t.Errorf("file = %q, want src/main.go", req.File)
	}
	if req.Function == nil || *req.Function != "Foo" {
		t.Errorf("function = %v, want Foo", req.Function)
	}
}

func TestRequestDeserializeShutdown(t *testing.T) {
	input := `{"protocol_version":"0.1.0","id":99,"command":"shutdown"}`
	var req Request
	if err := json.Unmarshal([]byte(input), &req); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if req.Command != "shutdown" {
		t.Errorf("command = %q, want shutdown", req.Command)
	}
	if req.ID != 99 {
		t.Errorf("id = %d, want 99", req.ID)
	}
}

func TestSymExprRoundTripBinOp(t *testing.T) {
	expr := SymExpr{
		Kind: "bin_op",
		Op:   "eq",
		Left: &SymExpr{
			Kind: "param",
			Name: "x",
			Path: []string{},
		},
		Right: &SymExpr{
			Kind:  "const",
			Type:  "int",
			Value: float64(42),
		},
	}
	data, err := json.Marshal(expr)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var decoded SymExpr
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if decoded.Kind != "bin_op" {
		t.Errorf("kind = %q, want bin_op", decoded.Kind)
	}
	if decoded.Left == nil || decoded.Left.Name != "x" {
		t.Errorf("left.name = %v, want x", decoded.Left)
	}
	if decoded.Right == nil || decoded.Right.Value != float64(42) {
		t.Errorf("right.value = %v, want 42", decoded.Right)
	}
}
