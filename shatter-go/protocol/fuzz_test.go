package protocol

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
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
		reqJSON(6, "setup", `"file":"./s.ts","function":"fn1","mode":"per_function"`),
		reqJSON(7, "teardown", `"function":"fn1"`),
		reqJSON(8, "generate", `"file":"./g.wasm","name":"User","kind":"type_name"`),
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
		`{"protocol_version":"0.1.0","id":5,"command":"setup","file":"s.ts","function":"fn1","mode":"per_execution"}`,
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
