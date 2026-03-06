package protocol

import (
	"encoding/json"
	"testing"

	"pgregory.net/rapid"
)

// ---------------------------------------------------------------------------
// Generators
// ---------------------------------------------------------------------------

func genIdent() *rapid.Generator[string] {
	return rapid.StringMatching(`[a-zA-Z_][a-zA-Z0-9_]{0,12}`)
}

func genTypeInfoLeaf() *rapid.Generator[TypeInfo] {
	return rapid.Map(
		rapid.SampledFrom([]string{"int", "float", "str", "bool", "unknown"}),
		func(kind string) TypeInfo { return TypeInfo{Kind: kind} },
	)
}

func genTypeInfo(depth int) *rapid.Generator[TypeInfo] {
	if depth <= 0 {
		return genTypeInfoLeaf()
	}
	return rapid.OneOf(
		genTypeInfoLeaf(),
		rapid.Map(genTypeInfo(depth-1), func(elem TypeInfo) TypeInfo {
			return TypeInfo{Kind: "array", Element: &elem}
		}),
		rapid.Map(genTypeInfo(depth-1), func(inner TypeInfo) TypeInfo {
			return TypeInfo{Kind: "nullable", Inner: &inner}
		}),
		rapid.Map(
			rapid.SliceOfN(genTypeInfo(depth-1), 2, 3),
			func(variants []TypeInfo) TypeInfo {
				return TypeInfo{Kind: "union", Variants: variants}
			},
		),
		rapid.Custom[TypeInfo](func(t *rapid.T) TypeInfo {
			return TypeInfo{Kind: "opaque", Label: genIdent().Draw(t, "label")}
		}),
	)
}

func genParamInfo() *rapid.Generator[ParamInfo] {
	return rapid.Custom[ParamInfo](func(t *rapid.T) ParamInfo {
		return ParamInfo{
			Name: genIdent().Draw(t, "name"),
			Type: genTypeInfo(2).Draw(t, "type"),
		}
	})
}

func genBranchInfo() *rapid.Generator[BranchInfo] {
	return rapid.Custom[BranchInfo](func(t *rapid.T) BranchInfo {
		return BranchInfo{
			ID:            rapid.IntRange(0, 100).Draw(t, "id"),
			Line:          rapid.IntRange(1, 500).Draw(t, "line"),
			ConditionText: rapid.StringMatching(`.{0,20}`).Draw(t, "cond"),
			Condition:     nil,
			BranchType:    rapid.SampledFrom([]string{"if", "else_if", "switch", "ternary", "logical_and", "logical_or", "while", "for"}).Draw(t, "bt"),
		}
	})
}

func genFunctionAnalysis() *rapid.Generator[FunctionAnalysis] {
	return rapid.Custom[FunctionAnalysis](func(t *rapid.T) FunctionAnalysis {
		startLine := rapid.IntRange(1, 500).Draw(t, "start")
		endLine := rapid.IntRange(startLine, startLine+200).Draw(t, "end")
		return FunctionAnalysis{
			Name:         genIdent().Draw(t, "name"),
			Exported:     rapid.Bool().Draw(t, "exported"),
			Params:       rapid.SliceOfN(genParamInfo(), 0, 4).Draw(t, "params"),
			Branches:     rapid.SliceOfN(genBranchInfo(), 0, 4).Draw(t, "branches"),
			Dependencies: []ExternalDependency{},
			ReturnType:   genTypeInfo(1).Draw(t, "retType"),
			StartLine:    startLine,
			EndLine:      endLine,
		}
	})
}

func genResponse() *rapid.Generator[Response] {
	return rapid.OneOf(
		rapid.Custom[Response](func(t *rapid.T) Response {
			return Response{
				ProtocolVersion: ProtocolVersion,
				ID:              rapid.IntRange(0, 1000).Draw(t, "id"),
				Status:          "handshake",
				FrontendVersion: ProtocolVersion,
				Language:        "go",
				Capabilities:    rapid.SliceOfN(genIdent(), 0, 3).Draw(t, "caps"),
			}
		}),
		rapid.Custom[Response](func(t *rapid.T) Response {
			return Response{
				ProtocolVersion: ProtocolVersion,
				ID:              rapid.IntRange(0, 1000).Draw(t, "id"),
				Status:          "analyze",
				Functions:       rapid.SliceOfN(genFunctionAnalysis(), 0, 3).Draw(t, "fns"),
			}
		}),
		rapid.Custom[Response](func(t *rapid.T) Response {
			return Response{
				ProtocolVersion: ProtocolVersion,
				ID:              rapid.IntRange(0, 1000).Draw(t, "id"),
				Status:          "error",
				Code:            rapid.SampledFrom([]string{"file_not_found", "internal_error", "parse_error"}).Draw(t, "code"),
				Message:         rapid.StringMatching(`.{0,30}`).Draw(t, "msg"),
			}
		}),
		rapid.Custom[Response](func(t *rapid.T) Response {
			return Response{
				ProtocolVersion: ProtocolVersion,
				ID:              rapid.IntRange(0, 1000).Draw(t, "id"),
				Status:          "shutdown_ack",
			}
		}),
	)
}

// ---------------------------------------------------------------------------
// Property tests
// ---------------------------------------------------------------------------

func TestPropertyTypeInfoRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		ti := genTypeInfo(3).Draw(t, "typeInfo")
		data, err := json.Marshal(ti)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		var decoded TypeInfo
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		if decoded.Kind != ti.Kind {
			t.Fatalf("kind mismatch: got %q, want %q", decoded.Kind, ti.Kind)
		}
	})
}

func TestPropertyResponseRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		resp := genResponse().Draw(t, "response")
		data, err := json.Marshal(resp)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		var decoded Response
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		if decoded.ProtocolVersion != resp.ProtocolVersion {
			t.Fatalf("protocol_version: got %q, want %q", decoded.ProtocolVersion, resp.ProtocolVersion)
		}
		if decoded.ID != resp.ID {
			t.Fatalf("id: got %d, want %d", decoded.ID, resp.ID)
		}
		if decoded.Status != resp.Status {
			t.Fatalf("status: got %q, want %q", decoded.Status, resp.Status)
		}
	})
}

func TestPropertyFunctionAnalysisRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		fa := genFunctionAnalysis().Draw(t, "fa")
		data, err := json.Marshal(fa)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		var decoded FunctionAnalysis
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		if decoded.Name != fa.Name {
			t.Fatalf("name: got %q, want %q", decoded.Name, fa.Name)
		}
		if len(decoded.Params) != len(fa.Params) {
			t.Fatalf("params count: got %d, want %d", len(decoded.Params), len(fa.Params))
		}
	})
}

func TestPropertyParamInfoRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		pi := genParamInfo().Draw(t, "param")
		data, err := json.Marshal(pi)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		var decoded ParamInfo
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		if decoded.Name != pi.Name {
			t.Fatalf("name: got %q, want %q", decoded.Name, pi.Name)
		}
		if decoded.Type.Kind != pi.Type.Kind {
			t.Fatalf("type kind: got %q, want %q", decoded.Type.Kind, pi.Type.Kind)
		}
	})
}
