package protocol

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"strings"
	"testing"
)

// FuzzProtocolParse exercises the full request-parsing pipeline: arbitrary
// bytes are fed as a single NDJSON line to the handler, which must never
// panic regardless of input shape.
func FuzzProtocolParse(f *testing.F) {
	// Valid protocol messages as seed corpus
	seeds := []string{
		reqJSON(1, "handshake", `"capabilities":["analyze","execute"]`),
		reqJSON(2, "analyze", `"file":"src/main.go","function":"Foo"`),
		reqJSON(3, "instrument", `"file":"src/main.go"`),
		reqJSON(4, "execute", `"file":"src/main.go","function":"Foo","inputs":[1,2]`),
		reqJSON(5, "shutdown"),
		reqJSON(6, "setup", `"file":"./s.ts","scope":"fn1","level":"function"`),
		reqJSON(7, "teardown", `"scope":"fn1","level":"function"`),
		reqJSON(8, "generate", `"file":"./g.wasm","name":"User","kind":"type_name"`),
		reqJSON(9, "prepare", `"file":"src/main.go","function":"Foo","mocks":[]`),
		reqJSON(10, "execute", `"function":"Foo","inputs":[1],"mocks":[],"prepare_id":"a1b2c3d4e5f6a7b8"`),
		// Edge cases
		`{}`,
		`[]`,
		`""`,
		`null`,
		`{"protocol_version":"99.0.0","id":1,"command":"handshake"}`,
		fmt.Sprintf(`{"protocol_version":%q,"id":0,"command":""}`, ProtocolVersion),
		`{"protocol_version":"0.1.0","id":-1,"command":"analyze","file":""}`,
		`not json at all`,
		`{"id":999999999999999999}`,
	}

	for _, s := range seeds {
		f.Add([]byte(s))
	}

	f.Fuzz(func(t *testing.T, data []byte) {
		input := bytes.NewReader(append(data, '\n'))
		var output bytes.Buffer
		handler := NewHandler(input, &output, io.Discard)
		// Must not panic — errors are handled gracefully via protocol responses
		handler.Run() //nolint:errcheck
	})
}

// FuzzRequestDeserialize fuzzes json.Unmarshal into Request directly.
func FuzzRequestDeserialize(f *testing.F) {
	seeds := []string{
		`{"protocol_version":"0.1.0","id":1,"command":"handshake","capabilities":["analyze"]}`,
		`{"protocol_version":"0.1.0","id":2,"command":"analyze","file":"main.go","function":"Foo"}`,
		`{"protocol_version":"0.1.0","id":3,"command":"execute","file":"x.go","function":"F","inputs":[1,"hello",true,null]}`,
		`{"protocol_version":"0.1.0","id":4,"command":"generate","file":"g.wasm","name":"User","kind":"type_name","recipe":{"seed":42}}`,
		`{"protocol_version":"0.1.0","id":5,"command":"setup","file":"s.ts","scope":"fn1","level":"execution"}`,
		`{"protocol_version":"0.1.0","id":6,"command":"prepare","file":"src/main.go","function":"Foo","mocks":[]}`,
		`{"protocol_version":"0.1.0","id":7,"command":"execute","function":"Foo","inputs":[1],"mocks":[],"prepare_id":"a1b2c3d4e5f6a7b8"}`,
		`{}`,
		`{"command":null}`,
		`{"id":"not_a_number"}`,
	}

	for _, s := range seeds {
		f.Add([]byte(s))
	}

	f.Fuzz(func(t *testing.T, data []byte) {
		var req Request
		// Must not panic — invalid JSON returns an error, not a crash
		json.Unmarshal(data, &req) //nolint:errcheck
	})
}

// FuzzResponseRoundTrip deserializes arbitrary bytes into Response,
// re-marshals, and re-deserializes to check for panics in the codec.
func FuzzResponseRoundTrip(f *testing.F) {
	seeds := []string{
		`{"protocol_version":"0.1.0","id":1,"status":"handshake","language":"go","capabilities":["analyze"]}`,
		`{"protocol_version":"0.1.0","id":2,"status":"error","code":"invalid_request","message":"bad"}`,
		`{"protocol_version":"0.1.0","id":3,"status":"execute","return_value":42,"branch_path":[{"branch_id":1,"line":10,"taken":true,"constraint":{"kind":"unknown"}}]}`,
		`{"protocol_version":"0.1.0","id":4,"status":"prepare","prepare_id":"a1b2c3d4e5f6a7b8"}`,
		`{}`,
	}

	for _, s := range seeds {
		f.Add([]byte(s))
	}

	f.Fuzz(func(t *testing.T, data []byte) {
		var resp Response
		if err := json.Unmarshal(data, &resp); err != nil {
			return
		}
		remarshaled, err := json.Marshal(resp)
		if err != nil {
			return
		}
		var resp2 Response
		json.Unmarshal(remarshaled, &resp2) //nolint:errcheck
	})
}

// FuzzTypeInfoDeserialize fuzzes the recursive TypeInfo type which has
// Element, Inner, Fields, and Variants sub-types.
func FuzzTypeInfoDeserialize(f *testing.F) {
	seeds := []string{
		`{"kind":"int"}`,
		`{"kind":"str"}`,
		`{"kind":"bool"}`,
		`{"kind":"float"}`,
		`{"kind":"unknown"}`,
		`{"kind":"opaque","label":"chan int"}`,
		`{"kind":"array","element":{"kind":"str"}}`,
		`{"kind":"nullable","inner":{"kind":"int"}}`,
		`{"kind":"object","fields":[["id",{"kind":"int"}],["name",{"kind":"str"}]]}`,
		`{"kind":"union","variants":[{"kind":"int"},{"kind":"str"}]}`,
		`{"kind":"complex","complex_kind":"date","inner":{"kind":"str"}}`,
		`{}`,
		`{"kind":""}`,
	}

	for _, s := range seeds {
		f.Add([]byte(s))
	}

	f.Fuzz(func(t *testing.T, data []byte) {
		var ti TypeInfo
		if err := json.Unmarshal(data, &ti); err != nil {
			return
		}
		// Round-trip must not panic
		remarshaled, err := json.Marshal(ti)
		if err != nil {
			return
		}
		var ti2 TypeInfo
		json.Unmarshal(remarshaled, &ti2) //nolint:errcheck
	})
}

// FuzzObjectFieldDeserialize fuzzes the custom [name, type] tuple format.
func FuzzObjectFieldDeserialize(f *testing.F) {
	seeds := []string{
		`["age",{"kind":"int"}]`,
		`["name",{"kind":"str"}]`,
		`["items",{"kind":"array","element":{"kind":"int"}}]`,
		`[]`,
		`[1,2]`,
		`"not an array"`,
		`null`,
	}

	for _, s := range seeds {
		f.Add([]byte(s))
	}

	f.Fuzz(func(t *testing.T, data []byte) {
		var field ObjectField
		if err := json.Unmarshal(data, &field); err != nil {
			return
		}
		remarshaled, err := json.Marshal(field)
		if err != nil {
			return
		}
		var field2 ObjectField
		json.Unmarshal(remarshaled, &field2) //nolint:errcheck
	})
}

// FuzzSetupContextStackDeserialize fuzzes the SetupContextStack type which
// carries layered setup state through the protocol.
func FuzzSetupContextStackDeserialize(f *testing.F) {
	seeds := []string{
		`{"contexts":[]}`,
		`{"contexts":[{"level":"session","context":{"id":"s1"}}]}`,
		`{"contexts":[{"level":"session","context":{}},{"level":"function","context":{"db":"test"}}]}`,
		`{}`,
		`{"contexts":null}`,
		`{"contexts":[{"level":"bogus","context":null}]}`,
	}
	for _, s := range seeds {
		f.Add([]byte(s))
	}
	f.Fuzz(func(t *testing.T, data []byte) {
		var stack SetupContextStack
		if err := json.Unmarshal(data, &stack); err != nil {
			return
		}
		remarshaled, err := json.Marshal(stack)
		if err != nil {
			return
		}
		var stack2 SetupContextStack
		json.Unmarshal(remarshaled, &stack2) //nolint:errcheck
	})
}

// FuzzSymExprDeserialize fuzzes the recursive SymExpr tree.
func FuzzSymExprDeserialize(f *testing.F) {
	seeds := []string{
		`{"kind":"param","name":"x","path":[]}`,
		`{"kind":"const","type":"int","value":42}`,
		`{"kind":"bin_op","op":"eq","left":{"kind":"param","name":"x","path":[]},"right":{"kind":"const","type":"int","value":0}}`,
		`{"kind":"unary_op","op":"not","operand":{"kind":"param","name":"flag","path":[]}}`,
		`{"kind":"method_call","receiver":{"kind":"param","name":"s","path":[]},"op":"indexOf","args":[{"kind":"const","type":"str","value":"@"}]}`,
		`{}`,
		`{"kind":"unknown"}`,
	}

	for _, s := range seeds {
		f.Add([]byte(s))
	}

	f.Fuzz(func(t *testing.T, data []byte) {
		var expr SymExpr
		if err := json.Unmarshal(data, &expr); err != nil {
			return
		}
		remarshaled, err := json.Marshal(expr)
		if err != nil {
			return
		}
		var expr2 SymExpr
		json.Unmarshal(remarshaled, &expr2) //nolint:errcheck
	})
}

// FuzzParamInfoDeserialize fuzzes ParamInfo which embeds the recursive TypeInfo.
func FuzzParamInfoDeserialize(f *testing.F) {
	seeds := []string{
		`{"name":"x","type":{"kind":"int"}}`,
		`{"name":"s","type":{"kind":"str"},"type_name":"string"}`,
		`{"name":"arr","type":{"kind":"array","element":{"kind":"float"}}}`,
		`{"name":"opt","type":{"kind":"nullable","inner":{"kind":"bool"}}}`,
		`{}`,
		`{"name":"","type":{}}`,
	}
	for _, s := range seeds {
		f.Add([]byte(s))
	}

	f.Fuzz(func(t *testing.T, data []byte) {
		var pi ParamInfo
		if err := json.Unmarshal(data, &pi); err != nil {
			return
		}
		remarshaled, err := json.Marshal(pi)
		if err != nil {
			return
		}
		var pi2 ParamInfo
		json.Unmarshal(remarshaled, &pi2) //nolint:errcheck
	})
}

// FuzzBranchInfoDeserialize fuzzes BranchInfo which contains SymExpr and
// SymConstraint recursive types.
func FuzzBranchInfoDeserialize(f *testing.F) {
	seeds := []string{
		`{"id":0,"line":5,"condition_text":"x > 0","branch_type":"if","condition":{"kind":"expression","expr":{"kind":"bin_op","op":"gt","left":{"kind":"param","name":"x","path":[]},"right":{"kind":"const","type":"int","value":0}}}}`,
		`{"id":1,"line":10,"condition_text":"flag","branch_type":"if","condition":{"kind":"unknown","hint":"opaque"}}`,
		`{"id":0,"line":1,"condition_text":"","branch_type":""}`,
		`{}`,
		`{"id":-1,"condition":null}`,
	}
	for _, s := range seeds {
		f.Add([]byte(s))
	}

	f.Fuzz(func(t *testing.T, data []byte) {
		var bi BranchInfo
		if err := json.Unmarshal(data, &bi); err != nil {
			return
		}
		remarshaled, err := json.Marshal(bi)
		if err != nil {
			return
		}
		var bi2 BranchInfo
		json.Unmarshal(remarshaled, &bi2) //nolint:errcheck
	})
}

// FuzzLiteralValueDeserialize fuzzes the LiteralValue tagged union which uses
// a "type" field to select between int/float/str/bool/regex variants.
func FuzzLiteralValueDeserialize(f *testing.F) {
	seeds := []string{
		`{"type":"int","value":42}`,
		`{"type":"float","value":3.14}`,
		`{"type":"str","value":"hello"}`,
		`{"type":"bool","value":true}`,
		`{"type":"regex","pattern":"\\d+"}`,
		`{}`,
		`{"type":""}`,
		`{"type":"unknown","value":null}`,
		`{"type":"int","value":"not_a_number"}`,
	}
	for _, s := range seeds {
		f.Add([]byte(s))
	}

	f.Fuzz(func(t *testing.T, data []byte) {
		var lv LiteralValue
		if err := json.Unmarshal(data, &lv); err != nil {
			return
		}
		remarshaled, err := json.Marshal(lv)
		if err != nil {
			return
		}
		var lv2 LiteralValue
		json.Unmarshal(remarshaled, &lv2) //nolint:errcheck
	})
}

// FuzzSideEffectDeserialize fuzzes the SideEffect type which has 7 possible
// kinds with different field combinations and json.RawMessage pointer fields.
func FuzzSideEffectDeserialize(f *testing.F) {
	seeds := []string{
		`{"kind":"console_output","level":"info","message":"hello"}`,
		`{"kind":"thrown_error","error_type":"TypeError","message":"bad","stack":"at main.go:5"}`,
		`{"kind":"file_write","path":"/tmp/x","content":"data"}`,
		`{"kind":"network_request","method":"GET","url":"http://example.com","body":{"key":"val"}}`,
		`{"kind":"global_mutation","name":"counter"}`,
		`{"kind":"global_state_change","variable":"cfg","before":null,"after":{"x":1}}`,
		`{"kind":"environment_read","name":"HOME","value":"/root"}`,
		`{}`,
		`{"kind":""}`,
		`{"kind":"console_output","body":null}`,
	}
	for _, s := range seeds {
		f.Add([]byte(s))
	}

	f.Fuzz(func(t *testing.T, data []byte) {
		var se SideEffect
		if err := json.Unmarshal(data, &se); err != nil {
			return
		}
		remarshaled, err := json.Marshal(se)
		if err != nil {
			return
		}
		var se2 SideEffect
		json.Unmarshal(remarshaled, &se2) //nolint:errcheck
	})
}

// FuzzMockConfigDeserialize fuzzes MockConfig directly with arbitrary JSON.
func FuzzMockConfigDeserialize(f *testing.F) {
	seeds := []string{
		`{"symbol":"fs.readFile","return_values":["ok"],"should_track_calls":true,"default_behavior":"repeat_last"}`,
		`{"symbol":"db.query","return_values":[null,{"rows":[]}],"should_track_calls":false,"default_behavior":"cycle"}`,
		`{"symbol":"","return_values":[],"should_track_calls":false,"default_behavior":""}`,
		`{}`,
		`{"symbol":"x","return_values":"not_array"}`,
	}
	for _, s := range seeds {
		f.Add([]byte(s))
	}

	f.Fuzz(func(t *testing.T, data []byte) {
		var mc MockConfig
		if err := json.Unmarshal(data, &mc); err != nil {
			return
		}
		remarshaled, err := json.Marshal(mc)
		if err != nil {
			return
		}
		var mc2 MockConfig
		json.Unmarshal(remarshaled, &mc2) //nolint:errcheck
	})
}

// FuzzFunctionAnalysisDeserialize fuzzes the top-level FunctionAnalysis
// composite type which contains ParamInfo, BranchInfo, LiteralValue, and
// other nested types.
func FuzzFunctionAnalysisDeserialize(f *testing.F) {
	seeds := []string{
		`{"name":"Foo","exported":true,"params":[{"name":"x","type":{"kind":"int"}}],"branches":[{"id":0,"line":5,"condition_text":"x>0","branch_type":"if"}],"dependencies":[],"return_type":{"kind":"str"},"start_line":1,"end_line":10}`,
		`{"name":"Bar","params":[],"branches":[],"dependencies":[{"kind":"import","symbol":"fmt.Println","source_module":"fmt","return_type":{"kind":"unknown"},"param_types":[],"call_sites":[3]}],"return_type":{"kind":"bool"},"start_line":1,"end_line":5,"literals":[{"type":"int","value":42}]}`,
		`{}`,
		`{"name":"","params":null,"branches":null,"dependencies":null,"return_type":{},"start_line":0,"end_line":0}`,
	}
	for _, s := range seeds {
		f.Add([]byte(s))
	}

	f.Fuzz(func(t *testing.T, data []byte) {
		var fa FunctionAnalysis
		if err := json.Unmarshal(data, &fa); err != nil {
			return
		}
		remarshaled, err := json.Marshal(fa)
		if err != nil {
			return
		}
		var fa2 FunctionAnalysis
		json.Unmarshal(remarshaled, &fa2) //nolint:errcheck
	})
}

// FuzzVersionParsing fuzzes parseMajorMinor and isVersionCompatible with
// arbitrary version strings.
func FuzzVersionParsing(f *testing.F) {
	seeds := []string{
		"0.1.0",
		"1.0.0",
		"99.99.99",
		"0.0.0",
		"",
		".",
		"..",
		"abc",
		"1",
		"1.",
		"1.2.3.4",
		"-1.0.0",
		"0.1.0-beta",
		"999999999999999999.0.0",
	}
	for _, s := range seeds {
		f.Add(s)
	}

	f.Fuzz(func(t *testing.T, version string) {
		if strings.ContainsRune(version, 0) {
			return
		}
		// Must not panic on any input
		parseMajorMinor(version)
		isVersionCompatible(version)
	})
}
