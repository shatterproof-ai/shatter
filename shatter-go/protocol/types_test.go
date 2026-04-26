package protocol

import (
	"encoding/json"
	"strings"
	"testing"
)

func strPtr(s string) *string { return &s }

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

func TestSymExprRoundTripIte(t *testing.T) {
	expr := SymExpr{
		Kind: "ite",
		Condition: &SymExpr{
			Kind: "param",
			Name: "flag",
			Path: []string{},
		},
		ThenExpr: &SymExpr{
			Kind: "param",
			Name: "b",
			Path: []string{},
		},
		ElseExpr: &SymExpr{
			Kind: "param",
			Name: "a",
			Path: []string{},
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
	if decoded.Kind != "ite" {
		t.Errorf("kind = %q, want ite", decoded.Kind)
	}
	if decoded.Condition == nil || decoded.Condition.Name != "flag" {
		t.Errorf("condition.name = %v, want flag", decoded.Condition)
	}
	if decoded.ThenExpr == nil || decoded.ThenExpr.Name != "b" {
		t.Errorf("then_expr.name = %v, want b", decoded.ThenExpr)
	}
	if decoded.ElseExpr == nil || decoded.ElseExpr.Name != "a" {
		t.Errorf("else_expr.name = %v, want a", decoded.ElseExpr)
	}
}

func TestFunctionAnalysisExportedField(t *testing.T) {
	tests := []struct {
		name     string
		exported bool
		wantKey  bool
	}{
		{"exported true", true, true},
		{"exported false (omitted)", false, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			fa := FunctionAnalysis{
				Name:     "Foo",
				Exported: tt.exported,
				Params:   []ParamInfo{},
			}
			data, err := json.Marshal(fa)
			if err != nil {
				t.Fatalf("marshal: %v", err)
			}
			got := string(data)
			hasKey := strings.Contains(got, `"exported"`)
			if hasKey != tt.wantKey {
				t.Errorf("exported key present=%v, want %v; json=%s", hasKey, tt.wantKey, got)
			}
			var decoded FunctionAnalysis
			if err := json.Unmarshal(data, &decoded); err != nil {
				t.Fatalf("unmarshal: %v", err)
			}
			if decoded.Exported != tt.exported {
				t.Errorf("exported = %v, want %v", decoded.Exported, tt.exported)
			}
		})
	}
}

func TestParamInfoWithTypeName(t *testing.T) {
	tests := []struct {
		name     string
		typeName *string
		wantKey  bool
	}{
		{"with type_name", strPtr("User"), true},
		{"without type_name", nil, false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			p := ParamInfo{
				Name:     "user",
				Type:     TypeInfo{Kind: "object"},
				TypeName: tt.typeName,
			}
			data, err := json.Marshal(p)
			if err != nil {
				t.Fatalf("marshal: %v", err)
			}
			got := string(data)
			hasKey := strings.Contains(got, `"type_name"`)
			if hasKey != tt.wantKey {
				t.Errorf("type_name key present=%v, want %v; json=%s", hasKey, tt.wantKey, got)
			}
			var decoded ParamInfo
			if err := json.Unmarshal(data, &decoded); err != nil {
				t.Fatalf("unmarshal: %v", err)
			}
			if tt.typeName != nil {
				if decoded.TypeName == nil || *decoded.TypeName != *tt.typeName {
					t.Errorf("type_name = %v, want %v", decoded.TypeName, *tt.typeName)
				}
			} else if decoded.TypeName != nil {
				t.Errorf("type_name = %v, want nil", decoded.TypeName)
			}
		})
	}
}

func TestErrorInfoNullableStack(t *testing.T) {
	tests := []struct {
		name  string
		stack *string
		want  string
	}{
		{"with stack", strPtr("goroutine 1 [running]:"), `"stack":"goroutine 1 [running]:`},
		{"null stack", nil, `"stack":null`},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			e := ErrorInfo{
				ErrorType: "panic",
				Message:   "oops",
				Stack:     tt.stack,
			}
			data, err := json.Marshal(e)
			if err != nil {
				t.Fatalf("marshal: %v", err)
			}
			got := string(data)
			if !strings.Contains(got, tt.want) {
				t.Errorf("json = %s, want to contain %s", got, tt.want)
			}
			var decoded ErrorInfo
			if err := json.Unmarshal(data, &decoded); err != nil {
				t.Fatalf("unmarshal: %v", err)
			}
			if tt.stack != nil {
				if decoded.Stack == nil || *decoded.Stack != *tt.stack {
					t.Errorf("stack = %v, want %v", decoded.Stack, *tt.stack)
				}
			} else if decoded.Stack != nil {
				t.Errorf("stack = %v, want nil", decoded.Stack)
			}
		})
	}
}

func TestSideEffectVariants(t *testing.T) {
	tests := []struct {
		name string
		se   SideEffect
		want string
	}{
		{"console_output", SideEffect{Kind: "console_output", Level: "info", Message: "hello"}, `"kind":"console_output"`},
		{"file_write", SideEffect{Kind: "file_write", Path: "/tmp/f", Content: "data"}, `"content":"data"`},
		{"network_request", SideEffect{Kind: "network_request", Method: "GET", URL: "http://x"}, `"kind":"network_request"`},
		{"environment_read", SideEffect{Kind: "environment_read", Variable: "HOME", Value: strPtr("/home/user")}, `"variable":"HOME"`},
		{"global_mutation", SideEffect{Kind: "global_mutation", Name: "counter"}, `"kind":"global_mutation"`},
		{"thrown_error", SideEffect{Kind: "thrown_error", ErrorType: "Error", Message: "bad", Stack: strPtr("trace")}, `"error_type":"Error"`},
		{"global_state_change", SideEffect{Kind: "global_state_change", Variable: "x"}, `"variable":"x"`},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			data, err := json.Marshal(tt.se)
			if err != nil {
				t.Fatalf("marshal: %v", err)
			}
			got := string(data)
			if !strings.Contains(got, tt.want) {
				t.Errorf("json = %s, want to contain %s", got, tt.want)
			}
			var decoded SideEffect
			if err := json.Unmarshal(data, &decoded); err != nil {
				t.Fatalf("unmarshal: %v", err)
			}
			if decoded.Kind != tt.se.Kind {
				t.Errorf("kind = %q, want %q", decoded.Kind, tt.se.Kind)
			}
		})
	}
}

func TestOutcomeStatusRoundTrip(t *testing.T) {
	statuses := []OutcomeStatus{
		OutcomeStatusCompleted,
		OutcomeStatusCompletedWithFindings,
		OutcomeStatusUnsupported,
		OutcomeStatusBuildFailed,
		OutcomeStatusRuntimeFailed,
		OutcomeStatusTimedOut,
		OutcomeStatusSkippedByPolicy,
	}
	for _, status := range statuses {
		data, err := json.Marshal(status)
		if err != nil {
			t.Fatalf("marshal %q: %v", status, err)
		}
		var decoded OutcomeStatus
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal %q: %v", status, err)
		}
		if decoded != status {
			t.Fatalf("decoded = %q, want %q", decoded, status)
		}
	}
}

func TestInvocationOutcomeRoundTrip(t *testing.T) {
	outcome := InvocationOutcome{
		Status:      OutcomeStatusCompletedWithFindings,
		ReturnValue: json.RawMessage(`{"ok":true}`),
		ThrownError: &ErrorInfo{
			ErrorType: "warning",
			Message:   "partial support",
		},
		SideEffects: []SideEffect{
			{Kind: "console_output", Level: "warn", Message: "degraded"},
		},
	}
	data, err := json.Marshal(outcome)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var decoded InvocationOutcome
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if decoded.Status != outcome.Status {
		t.Fatalf("status = %q, want %q", decoded.Status, outcome.Status)
	}
	if string(decoded.ReturnValue) != string(outcome.ReturnValue) {
		t.Fatalf("return_value = %s, want %s", decoded.ReturnValue, outcome.ReturnValue)
	}
	if decoded.ThrownError == nil || decoded.ThrownError.Message != outcome.ThrownError.Message {
		t.Fatalf("thrown_error = %#v, want %#v", decoded.ThrownError, outcome.ThrownError)
	}
	if len(decoded.SideEffects) != 1 || decoded.SideEffects[0].Kind != "console_output" {
		t.Fatalf("side_effects = %#v", decoded.SideEffects)
	}
}

// --- LiteralValue tests ---

func TestLiteralValue_RoundTrip(t *testing.T) {
	cases := []LiteralValue{
		{Type: "int", Value: int64(42)},
		{Type: "float", Value: 3.14},
		{Type: "str", Value: "express"},
		{Type: "bool", Value: true},
		{Type: "regex", Pattern: `\d+`},
	}
	for _, lit := range cases {
		data, err := json.Marshal(lit)
		if err != nil {
			t.Fatalf("marshal %s: %v", lit.Type, err)
		}
		var back LiteralValue
		if err := json.Unmarshal(data, &back); err != nil {
			t.Fatalf("unmarshal %s: %v", lit.Type, err)
		}
		if back.Type != lit.Type {
			t.Errorf("type = %q, want %q", back.Type, lit.Type)
		}
	}
}

func TestFunctionAnalysis_LiteralsRoundTrip(t *testing.T) {
	fa := FunctionAnalysis{
		Name:         "classify",
		Exported:     true,
		Params:       []ParamInfo{{Name: "s", Type: TypeInfo{Kind: "str"}}},
		Branches:     []BranchInfo{},
		Dependencies: []ExternalDependency{},
		ReturnType:   TypeInfo{Kind: "str"},
		StartLine:    1,
		EndLine:      10,
		Literals: []LiteralValue{
			{Type: "str", Value: "express"},
			{Type: "int", Value: int64(100)},
			{Type: "regex", Pattern: `\d{5}`},
		},
	}
	data, err := json.Marshal(fa)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var back FunctionAnalysis
	if err := json.Unmarshal(data, &back); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if len(back.Literals) != 3 {
		t.Errorf("literals len = %d, want 3", len(back.Literals))
	}
}

func TestFunctionAnalysis_EmptyLiteralsOmitted(t *testing.T) {
	fa := FunctionAnalysis{
		Name:         "stub",
		Params:       []ParamInfo{},
		Branches:     []BranchInfo{},
		Dependencies: []ExternalDependency{},
		ReturnType:   TypeInfo{Kind: "unknown"},
		StartLine:    1,
		EndLine:      1,
	}
	data, _ := json.Marshal(fa)
	var raw map[string]any
	json.Unmarshal(data, &raw)
	if _, hasLiterals := raw["literals"]; hasLiterals {
		t.Error("empty literals should be omitted from JSON")
	}
}

func TestCryptoBoundaryRoundTrip(t *testing.T) {
	cb := CryptoBoundary{
		Symbol:       "createDecipheriv",
		SourceModule: "crypto",
		Direction:    "decrypt",
		Output:       "plaintext",
		Confidence:   "high",
		ParamRoles:   map[string]string{"0": "algorithm", "1": "key", "2": "iv"},
		CallSites:    []int{5, 12},
	}
	data, err := json.Marshal(cb)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var decoded CryptoBoundary
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if decoded.Symbol != cb.Symbol {
		t.Errorf("symbol = %q, want %q", decoded.Symbol, cb.Symbol)
	}
	if decoded.Direction != cb.Direction {
		t.Errorf("direction = %q, want %q", decoded.Direction, cb.Direction)
	}
	if decoded.Confidence != cb.Confidence {
		t.Errorf("confidence = %q, want %q", decoded.Confidence, cb.Confidence)
	}
	if len(decoded.ParamRoles) != 3 {
		t.Errorf("param_roles len = %d, want 3", len(decoded.ParamRoles))
	}
}

func TestCryptoBoundaryHeuristicOmitsOutput(t *testing.T) {
	cb := CryptoBoundary{
		Symbol:       "encryptPayload",
		SourceModule: "my-custom-lib",
		Direction:    "encrypt",
		Confidence:   "medium",
		CallSites:    []int{42},
	}
	data, err := json.Marshal(cb)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var raw map[string]any
	if err := json.Unmarshal(data, &raw); err != nil {
		t.Fatalf("unmarshal raw: %v", err)
	}
	if _, has := raw["output"]; has {
		t.Error("empty output should be omitted from JSON")
	}
	var decoded CryptoBoundary
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if decoded.Confidence != "medium" {
		t.Errorf("confidence = %q, want %q", decoded.Confidence, "medium")
	}
}

func TestFunctionAnalysisCryptoBoundariesOmittedWhenEmpty(t *testing.T) {
	fa := FunctionAnalysis{
		Name:         "stub",
		Params:       []ParamInfo{},
		Branches:     []BranchInfo{},
		Dependencies: []ExternalDependency{},
		ReturnType:   TypeInfo{Kind: "unknown"},
		StartLine:    1,
		EndLine:      1,
	}
	data, _ := json.Marshal(fa)
	var raw map[string]any
	json.Unmarshal(data, &raw)
	if _, has := raw["crypto_boundaries"]; has {
		t.Error("empty crypto_boundaries should be omitted from JSON")
	}
}

func TestFunctionAnalysisCryptoBoundariesRoundTrip(t *testing.T) {
	fa := FunctionAnalysis{
		Name:         "decrypt",
		Params:       []ParamInfo{},
		Branches:     []BranchInfo{},
		Dependencies: []ExternalDependency{},
		ReturnType:   TypeInfo{Kind: "str"},
		StartLine:    1,
		EndLine:      10,
		CryptoBoundaries: []CryptoBoundary{{
			Symbol:       "createDecipheriv",
			SourceModule: "crypto",
			Direction:    "decrypt",
			Output:       "plaintext",
			Confidence:   "high",
			CallSites:    []int{3},
		}},
	}
	data, err := json.Marshal(fa)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var decoded FunctionAnalysis
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if len(decoded.CryptoBoundaries) != 1 {
		t.Fatalf("crypto_boundaries len = %d, want 1", len(decoded.CryptoBoundaries))
	}
	if decoded.CryptoBoundaries[0].Symbol != "createDecipheriv" {
		t.Errorf("symbol = %q, want createDecipheriv", decoded.CryptoBoundaries[0].Symbol)
	}
}

func TestExecuteResponseLoopBodyStatesRoundTrip(t *testing.T) {
	resp := Response{
		ProtocolVersion: "0.1.0",
		ID:              7,
		Status:          "execute",
		ReturnValue:     json.RawMessage(`42`),
		BranchPath:      []BranchDecision{},
		LinesExecuted:   []int{1, 2},
		CallsToExternal: []ExternalCall{},
		PathConstraints: []SymConstraint{},
		SideEffects:     []SideEffect{},
		ScopeEvents:     []json.RawMessage{},
		LoopBodyStates: []LoopBodyState{{
			LoopID:    0,
			Iteration: 1,
			Locals: map[string]SymExpr{
				"i": {
					Kind:  "const",
					Type:  "int",
					Value: float64(1),
				},
			},
		}},
		Performance: &PerfMetrics{WallTimeMs: 1},
	}

	data, err := json.Marshal(resp)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}

	var decoded Response
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}

	if len(decoded.LoopBodyStates) != 1 {
		t.Fatalf("loop_body_states len = %d, want 1", len(decoded.LoopBodyStates))
	}
	if decoded.LoopBodyStates[0].LoopID != 0 {
		t.Errorf("loop_id = %d, want 0", decoded.LoopBodyStates[0].LoopID)
	}
	if decoded.LoopBodyStates[0].Iteration != 1 {
		t.Errorf("iteration = %d, want 1", decoded.LoopBodyStates[0].Iteration)
	}
	if decoded.LoopBodyStates[0].Locals["i"].Kind != "const" {
		t.Errorf("locals[i].kind = %q, want const", decoded.LoopBodyStates[0].Locals["i"].Kind)
	}
}

// -----------------------------------------------------------------------------
// get_invocation_plan request / invocation_plan response (str-zbyp).
//
// The per-plan struct round-trips are covered in invocation_plan_test.go;
// these tests cover the transport envelope — Request.InvocationRequirements
// and Response.InvocationPlans / UnsatisfiedRequirements — plus a cross-
// language JSON fixture that shatter-core must also accept verbatim.
// -----------------------------------------------------------------------------

func TestRequestGetInvocationPlanRoundTrip(t *testing.T) {
	req := Request{
		ProtocolVersion: "0.1.0",
		ID:              42,
		Command:         "get_invocation_plan",
		InvocationRequirements: []InvocationRequirement{
			{
				TargetID: "example.com/pkg:Add",
				ValueRequirements: []ValueRequirement{
					{
						ParamIndex: 0,
						ParamName:  "x",
						TypeName:   "int",
						Kind:       ValueRequirementKindAny,
					},
					{
						ParamIndex: 1,
						ParamName:  "y",
						TypeName:   "int",
						Kind:       ValueRequirementKindSpecific,
						Literal:    json.RawMessage(`42`),
					},
				},
				RuntimeRequirements: []RuntimeRequirement{
					{
						Kind:     RuntimeRequirementKindReceiverConstruction,
						TypeName: "Counter",
						Detail:   "needs new Counter",
					},
				},
			},
		},
	}

	data, err := json.Marshal(req)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if !strings.Contains(string(data), `"command":"get_invocation_plan"`) {
		t.Errorf("missing command tag in %s", data)
	}

	var decoded Request
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if len(decoded.InvocationRequirements) != 1 {
		t.Fatalf("requirements len = %d, want 1", len(decoded.InvocationRequirements))
	}
	got := decoded.InvocationRequirements[0]
	if got.TargetID != "example.com/pkg:Add" {
		t.Errorf("target_id = %q", got.TargetID)
	}
	if len(got.ValueRequirements) != 2 {
		t.Fatalf("value_requirements len = %d, want 2", len(got.ValueRequirements))
	}
	if got.ValueRequirements[1].Kind != ValueRequirementKindSpecific {
		t.Errorf("kind = %q, want specific", got.ValueRequirements[1].Kind)
	}
	if len(got.RuntimeRequirements) != 1 {
		t.Fatalf("runtime_requirements len = %d, want 1", len(got.RuntimeRequirements))
	}
	if got.RuntimeRequirements[0].Kind != RuntimeRequirementKindReceiverConstruction {
		t.Errorf("runtime kind = %q", got.RuntimeRequirements[0].Kind)
	}
}

func TestResponseInvocationPlanRoundTrip(t *testing.T) {
	resp := Response{
		ProtocolVersion: "0.1.0",
		ID:              42,
		Status:          "invocation_plan",
		InvocationPlans: []InvocationPlan{
			{
				TargetID:     "example.com/pkg:Add",
				ReceiverKind: "constructor:NewCounter",
				ArgumentPlans: []ValuePlan{
					{
						ParamIndex: 0,
						ParamName:  "x",
						Kind:       ValuePlanKindLiteral,
						Literal:    json.RawMessage(`7`),
						TypeHint:   "int",
					},
					{
						ParamIndex: 1,
						ParamName:  "y",
						Kind:       ValuePlanKindSymbolic,
						TypeHint:   "int",
					},
				},
				Priority: 0,
				Label:    "constructor_new_counter",
			},
		},
		UnsatisfiedRequirements: []UnsatisfiedRequirement{
			{
				Kind:     UnsatisfiedRequirementKindCGODependency,
				TargetID: "example.com/pkg:Native",
				Detail:   "package uses cgo",
			},
		},
	}

	data, err := json.Marshal(resp)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if !strings.Contains(string(data), `"status":"invocation_plan"`) {
		t.Errorf("missing status tag in %s", data)
	}

	var decoded Response
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if len(decoded.InvocationPlans) != 1 {
		t.Fatalf("invocation_plans len = %d, want 1", len(decoded.InvocationPlans))
	}
	if decoded.InvocationPlans[0].Label != "constructor_new_counter" {
		t.Errorf("label = %q", decoded.InvocationPlans[0].Label)
	}
	if len(decoded.UnsatisfiedRequirements) != 1 {
		t.Fatalf("unsatisfied len = %d, want 1", len(decoded.UnsatisfiedRequirements))
	}
	if decoded.UnsatisfiedRequirements[0].Kind != UnsatisfiedRequirementKindCGODependency {
		t.Errorf("unsatisfied kind = %q", decoded.UnsatisfiedRequirements[0].Kind)
	}
}

func TestResponseInvocationPlanEmpty(t *testing.T) {
	// Empty plans and unsatisfied lists serialize via omitempty and still
	// deserialize cleanly.
	resp := Response{
		ProtocolVersion: "0.1.0",
		ID:              7,
		Status:          "invocation_plan",
	}
	data, err := json.Marshal(resp)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if strings.Contains(string(data), "invocation_plans") {
		t.Errorf("empty invocation_plans should be omitted: %s", data)
	}
	if strings.Contains(string(data), "unsatisfied_requirements") {
		t.Errorf("empty unsatisfied_requirements should be omitted: %s", data)
	}
	var decoded Response
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if decoded.Status != "invocation_plan" {
		t.Errorf("status = %q", decoded.Status)
	}
}

// TestInvocationPlanRustFixtureDeserializes verifies the Go side accepts a
// JSON payload shaped exactly as shatter-core/src/protocol.rs would emit it.
// This is the mirror of go_invocation_plan_fixture_deserializes in Rust: if
// either side drifts on a field name or kind spelling, one of the two tests
// fails.
func TestInvocationPlanRustFixtureDeserializes(t *testing.T) {
	rustJSON := `{
		"protocol_version": "0.1.0",
		"id": 42,
		"status": "invocation_plan",
		"invocation_plans": [
			{
				"target_id": "example.com/pkg:Add",
				"receiver_kind": "constructor:NewCounter",
				"argument_plans": [
					{
						"param_index": 0,
						"param_name": "x",
						"kind": "literal",
						"literal": 7,
						"type_hint": "int"
					},
					{
						"param_index": 1,
						"param_name": "y",
						"kind": "symbolic",
						"param_name": "y",
						"type_hint": "int"
					}
				],
				"priority": 0,
				"label": "constructor_new_counter"
			}
		],
		"unsatisfied_requirements": [
			{
				"kind": "cgo_dependency",
				"target_id": "example.com/pkg:Native",
				"detail": "package uses cgo"
			}
		]
	}`
	var resp Response
	if err := json.Unmarshal([]byte(rustJSON), &resp); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if resp.Status != "invocation_plan" {
		t.Errorf("status = %q", resp.Status)
	}
	if len(resp.InvocationPlans) != 1 {
		t.Fatalf("plans len = %d", len(resp.InvocationPlans))
	}
	plan := resp.InvocationPlans[0]
	if plan.TargetID != "example.com/pkg:Add" {
		t.Errorf("target_id = %q", plan.TargetID)
	}
	if len(plan.ArgumentPlans) != 2 {
		t.Fatalf("argument_plans len = %d", len(plan.ArgumentPlans))
	}
	if plan.ArgumentPlans[0].Kind != ValuePlanKindLiteral {
		t.Errorf("arg[0].kind = %q", plan.ArgumentPlans[0].Kind)
	}
	if plan.ArgumentPlans[1].Kind != ValuePlanKindSymbolic {
		t.Errorf("arg[1].kind = %q", plan.ArgumentPlans[1].Kind)
	}
	if resp.UnsatisfiedRequirements[0].Kind != UnsatisfiedRequirementKindCGODependency {
		t.Errorf("unsatisfied kind = %q", resp.UnsatisfiedRequirements[0].Kind)
	}
}

// TestRequestExecuteWithPlanRoundTrip locks the wire shape of an Execute
// request that carries an InvocationPlan (str-hy9b.H5). The optional `plan`
// field is the bridge from Rust core (planner_consumer output) into the Go
// launcher's receiver-aware dispatch path. Wire-output divergence here would
// break the H5 end-to-end pipeline.
func TestRequestExecuteWithPlanRoundTrip(t *testing.T) {
	function := "(*Service).DoIt"
	req := Request{
		ProtocolVersion: "0.1.0",
		ID:              23,
		Command:         "execute",
		Function:        &function,
		Inputs:          []json.RawMessage{json.RawMessage(`7`)},
		Plan: &InvocationPlan{
			TargetID:     "example.com/svc:(*Service).DoIt",
			ReceiverKind: "constructor:New",
			ArgumentPlans: []ValuePlan{
				{
					ParamIndex: 0,
					ParamName:  "x",
					Kind:       ValuePlanKindLiteral,
					Literal:    json.RawMessage(`7`),
					TypeHint:   "int",
				},
			},
			Priority: 0,
			Label:    "ctor_new",
		},
	}
	data, err := json.Marshal(req)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if !strings.Contains(string(data), `"plan":`) {
		t.Errorf("missing plan in serialized request: %s", data)
	}
	if !strings.Contains(string(data), `"receiver_kind":"constructor:New"`) {
		t.Errorf("missing receiver_kind in plan: %s", data)
	}
	var decoded Request
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if decoded.Plan == nil {
		t.Fatal("decoded.Plan is nil")
	}
	if decoded.Plan.ReceiverKind != "constructor:New" {
		t.Errorf("receiver_kind = %q", decoded.Plan.ReceiverKind)
	}
	if len(decoded.Plan.ArgumentPlans) != 1 {
		t.Fatalf("argument_plans len = %d", len(decoded.Plan.ArgumentPlans))
	}
}

// TestRequestExecuteWithoutPlanOmitsField guarantees the additive `plan`
// field stays omitempty — a free-function Execute request must serialize
// bit-identically to its pre-H5 shape.
func TestRequestExecuteWithoutPlanOmitsField(t *testing.T) {
	function := "Add"
	req := Request{
		ProtocolVersion: "0.1.0",
		ID:              24,
		Command:         "execute",
		Function:        &function,
		Inputs:          []json.RawMessage{json.RawMessage(`1`), json.RawMessage(`2`)},
	}
	data, err := json.Marshal(req)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if strings.Contains(string(data), `"plan":`) {
		t.Errorf("plan should be omitted when nil: %s", data)
	}
}
