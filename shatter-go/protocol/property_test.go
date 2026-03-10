package protocol

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
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
				Code:            rapid.SampledFrom([]string{"file_not_found", "internal_error", "parse_error", "compilation_error"}).Draw(t, "code"),
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
		rapid.Custom[Response](func(t *rapid.T) Response {
			return Response{
				ProtocolVersion: ProtocolVersion,
				ID:              rapid.IntRange(0, 1000).Draw(t, "id"),
				Status:          "teardown_ack",
			}
		}),
	)
}


func genSetupLevel() *rapid.Generator[SetupLevel] {
	return rapid.SampledFrom(ValidSetupLevels)
}

func genSetupContextEntry() *rapid.Generator[SetupContextEntry] {
	return rapid.Custom[SetupContextEntry](func(t *rapid.T) SetupContextEntry {
		level := genSetupLevel().Draw(t, "level")
		ctx := fmt.Sprintf(`{"key":"%s"}`, genIdent().Draw(t, "ctxKey"))
		raw := json.RawMessage(ctx)
		return SetupContextEntry{Level: level, Context: &raw}
	})
}

func genSetupContextStack() *rapid.Generator[SetupContextStack] {
	return rapid.Custom[SetupContextStack](func(t *rapid.T) SetupContextStack {
		n := rapid.IntRange(0, 4).Draw(t, "numContexts")
		entries := make([]SetupContextEntry, n)
		for i := 0; i < n; i++ {
			entries[i] = genSetupContextEntry().Draw(t, fmt.Sprintf("entry%d", i))
		}
		return SetupContextStack{Contexts: entries}
	})
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
func TestPropertySetupLevelRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		level := genSetupLevel().Draw(t, "level")
		data, err := json.Marshal(level)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		var decoded SetupLevel
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		if decoded != level {
			t.Fatalf("level mismatch: got %q, want %q", decoded, level)
		}
	})
}

func TestPropertySetupContextStackRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		stack := genSetupContextStack().Draw(t, "stack")
		data, err := json.Marshal(stack)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		var decoded SetupContextStack
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		if len(decoded.Contexts) != len(stack.Contexts) {
			t.Fatalf("contexts count: got %d, want %d", len(decoded.Contexts), len(stack.Contexts))
		}
		for i := range stack.Contexts {
			if decoded.Contexts[i].Level != stack.Contexts[i].Level {
				t.Fatalf("contexts[%d].level: got %q, want %q", i, decoded.Contexts[i].Level, stack.Contexts[i].Level)
			}
		}
	})
}

func TestPropertySetupLevelIsValid(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		level := genSetupLevel().Draw(t, "level")
		if !level.IsValid() {
			t.Fatalf("generated level %q should be valid", level)
		}
	})
}

func TestPropertySetupLevelInvalidString(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		s := rapid.StringMatching(`[a-z]{1,10}`).Draw(t, "str")
		level := SetupLevel(s)
		// If it happens to match a valid level, skip
		for _, valid := range ValidSetupLevels {
			if level == valid {
				return
			}
		}
		if level.IsValid() {
			t.Fatalf("random string %q should not be a valid level", s)
		}
	})
}



// ---------------------------------------------------------------------------
// Semantic properties — handler ordering
// ---------------------------------------------------------------------------

// genNonShutdownCommand produces commands that don't require filesystem access
// and don't trigger shutdown. Each returns a valid response or structured error.
func genNonShutdownCommand() *rapid.Generator[string] {
	return rapid.SampledFrom([]string{
		"handshake",
		"bogus_command", // unknown → error response
	})
}

// TestPropertyHandlerResponseOrdering verifies that for any sequence of valid
// requests, the handler produces responses with matching IDs in the same order
// (no reordering, no drops). Shutdown terminates processing.
func TestPropertyHandlerResponseOrdering(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		// Generate 1-5 non-shutdown commands followed by shutdown
		n := rapid.IntRange(1, 5).Draw(t, "numCommands")
		var lines []string
		var expectedIDs []int

		for i := 0; i < n; i++ {
			cmd := genNonShutdownCommand().Draw(t, fmt.Sprintf("cmd%d", i))
			line := fmt.Sprintf(`{"protocol_version":%q,"id":%d,"command":%q}`, ProtocolVersion, i+1, cmd)
			lines = append(lines, line)
			expectedIDs = append(expectedIDs, i+1)
		}
		// Append shutdown
		shutdownID := n + 1
		lines = append(lines, fmt.Sprintf(`{"protocol_version":%q,"id":%d,"command":"shutdown"}`, ProtocolVersion, shutdownID))
		expectedIDs = append(expectedIDs, shutdownID)

		input := strings.Join(lines, "\n") + "\n"
		var out bytes.Buffer
		h := NewHandlerWithLogLevel(strings.NewReader(input), &out, &bytes.Buffer{}, "error")

		if err := h.Run(); err != nil {
			t.Fatalf("handler.Run(): %v", err)
		}

		// Parse responses
		scanner := bufio.NewScanner(&out)
		var gotIDs []int
		for scanner.Scan() {
			var resp Response
			if err := json.Unmarshal(scanner.Bytes(), &resp); err != nil {
				t.Fatalf("unmarshal response: %v", err)
			}
			gotIDs = append(gotIDs, resp.ID)
		}

		if len(gotIDs) != len(expectedIDs) {
			t.Fatalf("response count: got %d, want %d", len(gotIDs), len(expectedIDs))
		}
		for i := range expectedIDs {
			if gotIDs[i] != expectedIDs[i] {
				t.Fatalf("response[%d] ID: got %d, want %d", i, gotIDs[i], expectedIDs[i])
			}
		}
	})
}

// TestPropertyHandlerShutdownStopsProcessing verifies that requests after
// shutdown are not processed.
func TestPropertyHandlerShutdownStopsProcessing(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		// N commands after shutdown — none should produce responses
		nAfter := rapid.IntRange(1, 3).Draw(t, "afterCount")

		var lines []string
		lines = append(lines, fmt.Sprintf(`{"protocol_version":%q,"id":1,"command":"shutdown"}`, ProtocolVersion))
		for i := 0; i < nAfter; i++ {
			lines = append(lines, fmt.Sprintf(`{"protocol_version":%q,"id":%d,"command":"handshake"}`, ProtocolVersion, 100+i))
		}

		input := strings.Join(lines, "\n") + "\n"
		var out bytes.Buffer
		h := NewHandlerWithLogLevel(strings.NewReader(input), &out, &bytes.Buffer{}, "error")
		h.Run()

		scanner := bufio.NewScanner(&out)
		count := 0
		for scanner.Scan() {
			count++
		}
		// Only the shutdown_ack response should appear
		if count != 1 {
			t.Fatalf("expected 1 response (shutdown_ack), got %d", count)
		}
	})
}

// ---------------------------------------------------------------------------
// Semantic properties — analysis parameter extraction
// ---------------------------------------------------------------------------

// goTypeMap maps Go type names to the Shatter TypeInfo.Kind the analyzer produces.
var goTypeMap = map[string]string{
	"int":     "int",
	"int64":   "int",
	"float64": "float",
	"string":  "str",
	"bool":    "bool",
}

// TestPropertyAnalyzeExtractsCorrectParams generates simple Go functions with
// known signatures and verifies AnalyzeFile extracts the right parameter count,
// names, and types.
func TestPropertyAnalyzeExtractsCorrectParams(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		goTypes := []string{"int", "int64", "float64", "string", "bool"}

		nParams := rapid.IntRange(0, 4).Draw(t, "nParams")
		type paramSpec struct {
			name   string
			goType string
		}
		var params []paramSpec
		for i := 0; i < nParams; i++ {
			name := rapid.StringMatching(`[a-z][a-z0-9]{0,5}`).Draw(t, fmt.Sprintf("name%d", i))
			goType := rapid.SampledFrom(goTypes).Draw(t, fmt.Sprintf("type%d", i))
			params = append(params, paramSpec{name: name, goType: goType})
		}

		// Build source
		var paramList []string
		for _, p := range params {
			paramList = append(paramList, fmt.Sprintf("%s %s", p.name, p.goType))
		}
		src := fmt.Sprintf("package p\n\nfunc Foo(%s) int { return 0 }\n", strings.Join(paramList, ", "))

		// Write to temp file
		dir, err := os.MkdirTemp("", "shatter-go-prop-*")
		if err != nil {
			t.Fatalf("mkdirtemp: %v", err)
		}
		defer os.RemoveAll(dir)
		fpath := filepath.Join(dir, "test.go")
		if err := os.WriteFile(fpath, []byte(src), 0o644); err != nil {
			t.Fatalf("write temp file: %v", err)
		}

		results, err := AnalyzeFile(fpath, "Foo")
		if err != nil {
			t.Fatalf("AnalyzeFile: %v", err)
		}
		if len(results) != 1 {
			t.Fatalf("expected 1 function, got %d", len(results))
		}

		fa := results[0]
		if fa.Name != "Foo" {
			t.Fatalf("name: got %q, want %q", fa.Name, "Foo")
		}
		if len(fa.Params) != nParams {
			t.Fatalf("param count: got %d, want %d", len(fa.Params), nParams)
		}
		for i, p := range params {
			if fa.Params[i].Name != p.name {
				t.Fatalf("param[%d] name: got %q, want %q", i, fa.Params[i].Name, p.name)
			}
			expectedKind := goTypeMap[p.goType]
			if fa.Params[i].Type.Kind != expectedKind {
				t.Fatalf("param[%d] type: got %q, want %q (Go type %s)", i, fa.Params[i].Type.Kind, expectedKind, p.goType)
			}
		}
	})
}
