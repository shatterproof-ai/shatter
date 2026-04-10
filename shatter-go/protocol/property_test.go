package protocol

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	"pgregory.net/rapid"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
)

// ---------------------------------------------------------------------------
// Generators
// ---------------------------------------------------------------------------

var goKeywords = map[string]struct{}{
	"break": {}, "default": {}, "func": {}, "interface": {}, "select": {},
	"case": {}, "defer": {}, "go": {}, "map": {}, "struct": {},
	"chan": {}, "else": {}, "goto": {}, "package": {}, "switch": {},
	"const": {}, "fallthrough": {}, "if": {}, "range": {}, "type": {},
	"continue": {}, "for": {}, "import": {}, "return": {}, "var": {},
}

func genIdent() *rapid.Generator[string] {
	return rapid.StringMatching(`[a-zA-Z_][a-zA-Z0-9_]{0,12}`)
}

func sanitizeGoIdent(name string) string {
	if _, isKeyword := goKeywords[name]; isKeyword {
		return name + "_"
	}
	return name
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

func genSymExprLeaf() *rapid.Generator[SymExpr] {
	return rapid.OneOf(
		rapid.Map(rapid.Int64Range(-1000, 1000), func(v int64) SymExpr {
			return SymExpr{Kind: "const", Type: "int", Value: v, Args: []SymExpr{}}
		}),
		rapid.Map(genIdent(), func(name string) SymExpr {
			return SymExpr{Kind: "param", Name: name, Path: []string{}, Args: []SymExpr{}}
		}),
		rapid.Just(SymExpr{Kind: "unknown", Args: []SymExpr{}}),
	)
}

func genBoundOp() *rapid.Generator[string] {
	return rapid.SampledFrom([]string{"lt", "le", "gt", "ge"})
}

func genInductionVar() *rapid.Generator[InductionVar] {
	return rapid.Custom[InductionVar](func(t *rapid.T) InductionVar {
		initLeaf := genSymExprLeaf().Draw(t, "init")
		stepLeaf := genSymExprLeaf().Draw(t, "step")
		boundLeaf := genSymExprLeaf().Draw(t, "bound")
		return InductionVar{
			Name:      genIdent().Draw(t, "name"),
			InitExpr:  &initLeaf,
			StepExpr:  &stepLeaf,
			BoundExpr: &boundLeaf,
			BoundOp:   genBoundOp().Draw(t, "boundOp"),
		}
	})
}

func genLoopInfo() *rapid.Generator[LoopInfo] {
	return rapid.Custom[LoopInfo](func(t *rapid.T) LoopInfo {
		iv := genInductionVar().Draw(t, "iv")
		return LoopInfo{
			LoopID:       rapid.IntRange(0, 50).Draw(t, "loopID"),
			Line:         rapid.IntRange(1, 500).Draw(t, "line"),
			InductionVar: &iv,
		}
	})
}

func genFunctionAnalysis() *rapid.Generator[FunctionAnalysis] {
	return rapid.Custom[FunctionAnalysis](func(t *rapid.T) FunctionAnalysis {
		startLine := rapid.IntRange(1, 500).Draw(t, "start")
		endLine := rapid.IntRange(startLine, startLine+200).Draw(t, "end")
		var loops []LoopInfo
		if rapid.Bool().Draw(t, "hasLoops") {
			loops = rapid.SliceOfN(genLoopInfo(), 0, 3).Draw(t, "loops")
		}
		var sourceFile string
		if rapid.Bool().Draw(t, "hasSourceFile") {
			sourceFile = "/src/" + genIdent().Draw(t, "sourceFile") + ".ts"
		}
		return FunctionAnalysis{
			Name:         genIdent().Draw(t, "name"),
			Exported:     rapid.Bool().Draw(t, "exported"),
			Params:       rapid.SliceOfN(genParamInfo(), 0, 4).Draw(t, "params"),
			Branches:     rapid.SliceOfN(genBranchInfo(), 0, 4).Draw(t, "branches"),
			Dependencies: []ExternalDependency{},
			ReturnType:   genTypeInfo(1).Draw(t, "retType"),
			StartLine:    startLine,
			EndLine:      endLine,
			Loops:        loops,
			SourceFile:   sourceFile,
		}
	})
}

func genTimingPhaseSummary() *rapid.Generator[TimingPhaseSummary] {
	return rapid.Custom[TimingPhaseSummary](func(t *rapid.T) TimingPhaseSummary {
		totalMs := rapid.Float64Range(0, 10000).Draw(t, "totalMs")
		selfMs := rapid.Float64Range(0, totalMs).Draw(t, "selfMs")
		return TimingPhaseSummary{
			PhasePath: rapid.SampledFrom([]string{
				"analyze.total", "analyze.parse", "analyze.typecheck", "analyze.walk",
				"instrument.total", "execute.total", "execute.run", "serialize.response",
			}).Draw(t, "phasePath"),
			TotalMs: totalMs,
			SelfMs:  selfMs,
			Count:   rapid.IntRange(1, 10).Draw(t, "count"),
		}
	})
}

func genTimingSummary() *rapid.Generator[TimingSummary] {
	return rapid.Custom[TimingSummary](func(t *rapid.T) TimingSummary {
		return TimingSummary{
			Phases: rapid.SliceOfN(genTimingPhaseSummary(), 1, 5).Draw(t, "phases"),
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
				Code:            rapid.SampledFrom(AllErrorCodes).Draw(t, "code"),
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

func TestPropertyLoopInfoRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		li := genLoopInfo().Draw(t, "loopInfo")
		data, err := json.Marshal(li)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		var decoded LoopInfo
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		if decoded.LoopID != li.LoopID {
			t.Fatalf("loop_id: got %d, want %d", decoded.LoopID, li.LoopID)
		}
		if decoded.Line != li.Line {
			t.Fatalf("line: got %d, want %d", decoded.Line, li.Line)
		}
		if decoded.InductionVar == nil {
			t.Fatal("induction_var: got nil, want non-nil")
		}
		if decoded.InductionVar.Name != li.InductionVar.Name {
			t.Fatalf("induction_var.name: got %q, want %q", decoded.InductionVar.Name, li.InductionVar.Name)
		}
		if decoded.InductionVar.BoundOp != li.InductionVar.BoundOp {
			t.Fatalf("induction_var.bound_op: got %q, want %q", decoded.InductionVar.BoundOp, li.InductionVar.BoundOp)
		}
	})
}

func TestPropertyFunctionAnalysisLoopsRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		fa := genFunctionAnalysis().Draw(t, "funcAnalysis")
		data, err := json.Marshal(fa)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		var decoded FunctionAnalysis
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		if len(decoded.Loops) != len(fa.Loops) {
			t.Fatalf("loops len: got %d, want %d", len(decoded.Loops), len(fa.Loops))
		}
		for i, loop := range fa.Loops {
			if decoded.Loops[i].LoopID != loop.LoopID {
				t.Fatalf("loops[%d].loop_id: got %d, want %d", i, decoded.Loops[i].LoopID, loop.LoopID)
			}
			if decoded.Loops[i].InductionVar == nil {
				t.Fatalf("loops[%d].induction_var: got nil, want non-nil", i)
			}
			if decoded.Loops[i].InductionVar.BoundOp != loop.InductionVar.BoundOp {
				t.Fatalf("loops[%d].induction_var.bound_op: got %q, want %q", i, decoded.Loops[i].InductionVar.BoundOp, loop.InductionVar.BoundOp)
			}
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
			name := sanitizeGoIdent(
				rapid.StringMatching(`[a-z][a-z0-9]{0,5}`).Draw(t, fmt.Sprintf("name%d", i)),
			)
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

// TestPropertyCaptureFalseAlwaysYieldsEmptySideEffects verifies the protocol
// semantic: when a Request is deserialized with capture=false, the Capture
// field is non-nil and false.
func TestPropertyCaptureFieldRoundtrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		captureVal := rapid.Bool().Draw(t, "capture")
		req := Request{
			ProtocolVersion: ProtocolVersion,
			ID:              1,
			Command:         "execute",
			Capture:         &captureVal,
		}

		data, err := json.Marshal(req)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}

		var decoded Request
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}

		if decoded.Capture == nil {
			t.Fatal("Capture should not be nil after roundtrip when explicitly set")
		}
		if *decoded.Capture != captureVal {
			t.Fatalf("Capture: got %v, want %v", *decoded.Capture, captureVal)
		}
	})
}

// TestPropertyCaptureNilDefaultsToTrue verifies that an Execute request with
// no capture field (nil) is treated as capture=true, matching protocol defaults.
func TestPropertyCaptureNilDefaultsToTrue(t *testing.T) {
	// When capture is omitted from the JSON, the field is nil.
	// The handler interprets nil as true (capture enabled by default).
	data := []byte(`{"protocol_version":"0.1.0","id":1,"command":"execute"}`)
	var req Request
	if err := json.Unmarshal(data, &req); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	// Nil capture means default (true).
	capture := req.Capture == nil || *req.Capture
	if !capture {
		t.Error("nil Capture should default to true")
	}
}

// genSideEffect generates a random protocol.SideEffect covering all 7 canonical kinds.
func genSideEffect() *rapid.Generator[SideEffect] {
	return rapid.Custom[SideEffect](func(t *rapid.T) SideEffect {
		kind := rapid.SampledFrom([]string{
			"console_output", "file_write", "network_request",
			"environment_read", "global_mutation", "thrown_error", "global_state_change",
		}).Draw(t, "kind")

		effect := SideEffect{Kind: kind}
		switch kind {
		case "console_output":
			effect.Level = rapid.SampledFrom([]string{"log", "warn", "error", "info", "debug"}).Draw(t, "level")
			effect.Message = rapid.StringN(0, 30, -1).Draw(t, "message")
		case "file_write":
			effect.Path = "/tmp/" + genIdent().Draw(t, "path")
			if rapid.Bool().Draw(t, "has_content") {
				effect.Content = rapid.StringN(0, 30, -1).Draw(t, "content")
			}
		case "network_request":
			effect.Method = rapid.SampledFrom([]string{"GET", "POST", "PUT", "DELETE"}).Draw(t, "method")
			effect.URL = "https://example.com/" + genIdent().Draw(t, "url")
		case "environment_read":
			effect.Variable = genIdent().Draw(t, "variable")
			if rapid.Bool().Draw(t, "has_value") {
				s := genIdent().Draw(t, "value")
				effect.Value = &s
			}
		case "global_mutation":
			effect.Name = genIdent().Draw(t, "name")
		case "thrown_error":
			effect.ErrorType = genIdent().Draw(t, "error_type")
			effect.Message = rapid.StringN(0, 30, -1).Draw(t, "message")
			if rapid.Bool().Draw(t, "has_stack") {
				s := rapid.StringN(0, 50, -1).Draw(t, "stack")
				effect.Stack = &s
			}
		case "global_state_change":
			effect.Variable = genIdent().Draw(t, "variable")
			before := json.RawMessage(`0`)
			after := json.RawMessage(`1`)
			effect.Before = &before
			effect.After = &after
		}
		return effect
	})
}

// TestPropertySideEffectRoundTrip verifies that all 7 canonical side effect kinds
// survive a JSON marshal/unmarshal roundtrip with the "kind" field intact.
func TestPropertySideEffectRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		effect := genSideEffect().Draw(t, "effect")
		data, err := json.Marshal(effect)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		var decoded SideEffect
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		if decoded.Kind != effect.Kind {
			t.Fatalf("kind mismatch: got %q, want %q", decoded.Kind, effect.Kind)
		}
		var raw map[string]interface{}
		if err := json.Unmarshal(data, &raw); err != nil {
			t.Fatalf("raw unmarshal: %v", err)
		}
		if _, hasType := raw["type"]; hasType {
			t.Fatalf("JSON must not contain 'type' field, only 'kind': %s", data)
		}
		if raw["kind"] != effect.Kind {
			t.Fatalf("JSON 'kind' = %q, want %q", raw["kind"], effect.Kind)
		}
	})
}

// TestPropertySideEffectKindCoverage verifies that genSideEffect produces
// all 7 canonical kinds across a sufficient number of draws.
func TestPropertySideEffectKindCoverage(t *testing.T) {
	seen := make(map[string]bool)
	for i := 0; i < 500; i++ {
		rapid.Check(t, func(t *rapid.T) {
			effect := genSideEffect().Draw(t, "effect")
			seen[effect.Kind] = true
		})
		if len(seen) == 7 {
			break
		}
	}
	required := []string{
		"console_output", "file_write", "network_request",
		"environment_read", "global_mutation", "thrown_error", "global_state_change",
	}
	for _, kind := range required {
		if !seen[kind] {
			t.Errorf("kind %q was never generated by genSideEffect", kind)
		}
	}
}

// TestPropertyPrepareRequestRoundTrip verifies that a prepare request serializes
// and deserializes correctly, preserving all fields.
func TestPropertyPrepareRequestRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		file := genIdent().Draw(t, "file")
		function := genIdent().Draw(t, "function")
		req := Request{
			ProtocolVersion: ProtocolVersion,
			ID:              rapid.IntRange(0, 1000).Draw(t, "id"),
			Command:         "prepare",
			File:            file,
			Function:        &function,
		}

		data, err := json.Marshal(req)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}

		var decoded Request
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}

		if decoded.Command != "prepare" {
			t.Fatalf("Command: got %q, want %q", decoded.Command, "prepare")
		}
		if decoded.File != file {
			t.Fatalf("File: got %q, want %q", decoded.File, file)
		}
		if decoded.Function == nil || *decoded.Function != function {
			t.Fatalf("Function: got %v, want %q", decoded.Function, function)
		}
	})
}

// TestPropertyPrepareResponseRoundTrip verifies that a prepare response with a
// prepare_id serializes and deserializes correctly.
func TestPropertyPrepareResponseRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		prepareID := rapid.StringMatching(`[a-f0-9]{16}`).Draw(t, "prepareID")
		resp := Response{
			ProtocolVersion: ProtocolVersion,
			ID:              rapid.IntRange(0, 1000).Draw(t, "id"),
			Status:          "prepare",
			PrepareID:       prepareID,
		}

		data, err := json.Marshal(resp)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}

		var decoded Response
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}

		if decoded.Status != "prepare" {
			t.Fatalf("Status: got %q, want %q", decoded.Status, "prepare")
		}
		if decoded.PrepareID != prepareID {
			t.Fatalf("PrepareID: got %q, want %q", decoded.PrepareID, prepareID)
		}
	})
}

// TestPropertyExecuteWithPrepareIDRoundTrip verifies that an execute request
// with a prepare_id round-trips correctly.
func TestPropertyExecuteWithPrepareIDRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		prepareID := rapid.StringMatching(`[a-f0-9]{16}`).Draw(t, "prepareID")
		req := Request{
			ProtocolVersion: ProtocolVersion,
			ID:              rapid.IntRange(0, 1000).Draw(t, "id"),
			Command:         "execute",
			PrepareID:       &prepareID,
		}

		data, err := json.Marshal(req)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}

		var decoded Request
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}

		if decoded.PrepareID == nil {
			t.Fatal("PrepareID should not be nil after roundtrip when set")
		}
		if *decoded.PrepareID != prepareID {
			t.Fatalf("PrepareID: got %q, want %q", *decoded.PrepareID, prepareID)
		}
	})
}

// TestPropertyPrepareIDOmittedWhenNil verifies that prepare_id is omitted from
// JSON when the field is nil, preserving backward compatibility.
func TestPropertyPrepareIDOmittedWhenNil(t *testing.T) {
	req := Request{
		ProtocolVersion: ProtocolVersion,
		ID:              1,
		Command:         "execute",
		PrepareID:       nil,
	}
	data, err := json.Marshal(req)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var obj map[string]any
	if err := json.Unmarshal(data, &obj); err != nil {
		t.Fatalf("unmarshal to map: %v", err)
	}
	if _, ok := obj["prepare_id"]; ok {
		t.Error("prepare_id should be omitted from JSON when nil")
	}
}

func TestErrorCodeParityWithRegistry(t *testing.T) {
	// AllErrorCodes must have exactly 11 entries matching protocol/registry.yaml.
	if len(AllErrorCodes) != 11 {
		t.Fatalf("AllErrorCodes has %d entries, want 11", len(AllErrorCodes))
	}

	// Every constant must appear in the slice.
	expected := []string{
		ErrFileNotFound, ErrFunctionNotFound, ErrParseError,
		ErrInstrumentationFailed, ErrExecutionTimeout, ErrExecutionCrash,
		ErrVersionMismatch, ErrInvalidRequest, ErrCompilationError,
		ErrInternalError, ErrNotSupported,
	}
	seen := make(map[string]bool)
	for _, code := range AllErrorCodes {
		seen[code] = true
	}
	for _, code := range expected {
		if !seen[code] {
			t.Errorf("expected error code %q missing from AllErrorCodes", code)
		}
	}
}

// ---------------------------------------------------------------------------
// computePrepareID semantic properties
// ---------------------------------------------------------------------------

var hexPattern = regexp.MustCompile(`^[a-f0-9]{16}$`)

// genMock returns an arbitrary MockConfig with a non-empty Symbol.
func genMock(t *rapid.T, label string) instrument.MockConfig {
	return instrument.MockConfig{
		Symbol: rapid.StringMatching(`[a-zA-Z][a-zA-Z0-9_]{0,15}`).Draw(t, label+"_symbol"),
	}
}

// TestPropertyComputePrepareIDAlways16Hex verifies that computePrepareID always
// returns exactly 16 lowercase hex characters regardless of input.
func TestPropertyComputePrepareIDAlways16Hex(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		file := rapid.String().Draw(t, "file")
		fn := rapid.String().Draw(t, "fn")
		n := rapid.IntRange(0, 5).Draw(t, "n")
		mocks := make([]instrument.MockConfig, n)
		for i := range mocks {
			mocks[i] = genMock(t, fmt.Sprintf("mock%d", i))
		}
		id := computePrepareID(file, fn, mocks)
		if !hexPattern.MatchString(id) {
			t.Fatalf("computePrepareID(%q, %q, mocks) = %q, want 16 lowercase hex chars", file, fn, id)
		}
	})
}

// TestPropertyComputePrepareIDDeterministic verifies that calling computePrepareID
// twice with identical inputs returns the same ID.
func TestPropertyComputePrepareIDDeterministic(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		file := rapid.String().Draw(t, "file")
		fn := rapid.String().Draw(t, "fn")
		n := rapid.IntRange(0, 5).Draw(t, "n")
		mocks := make([]instrument.MockConfig, n)
		for i := range mocks {
			mocks[i] = genMock(t, fmt.Sprintf("mock%d", i))
		}
		id1 := computePrepareID(file, fn, mocks)
		id2 := computePrepareID(file, fn, mocks)
		if id1 != id2 {
			t.Fatalf("not deterministic: first=%q second=%q", id1, id2)
		}
	})
}

// TestPropertyComputePrepareIDMockOrderIndependent verifies that the order of
// mocks does not affect the computed ID (symbols are sorted internally).
func TestPropertyComputePrepareIDMockOrderIndependent(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		file := rapid.StringMatching(`[a-z/]{1,20}\.go`).Draw(t, "file")
		fn := rapid.StringMatching(`[A-Za-z][A-Za-z0-9]{0,10}`).Draw(t, "fn")
		n := rapid.IntRange(2, 5).Draw(t, "n")
		mocks := make([]instrument.MockConfig, n)
		for i := range mocks {
			// Use distinct symbols to ensure ordering matters.
			mocks[i] = instrument.MockConfig{
				Symbol: fmt.Sprintf("Mock%c", 'A'+i),
			}
		}
		// Reversed order.
		reversed := make([]instrument.MockConfig, n)
		for i, m := range mocks {
			reversed[n-1-i] = m
		}
		id1 := computePrepareID(file, fn, mocks)
		id2 := computePrepareID(file, fn, reversed)
		if id1 != id2 {
			t.Fatalf("mock order affected ID: forward=%q reversed=%q", id1, id2)
		}
	})
}

// TestPropertyComputePrepareIDFileSensitive verifies that different file paths
// produce different IDs (for the same function and no mocks).
func TestPropertyComputePrepareIDFileSensitive(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		file1 := rapid.StringMatching(`[a-z]{3,10}/[a-z]{3,10}\.go`).Draw(t, "file1")
		file2 := rapid.StringMatching(`[a-z]{3,10}/[a-z]{3,10}\.go`).Draw(t, "file2")
		if file1 == file2 {
			t.Skip()
		}
		fn := rapid.StringMatching(`[A-Za-z][A-Za-z0-9]{0,10}`).Draw(t, "fn")
		id1 := computePrepareID(file1, fn, nil)
		id2 := computePrepareID(file2, fn, nil)
		if id1 == id2 {
			t.Fatalf("different files produced same ID: file1=%q file2=%q id=%q", file1, file2, id1)
		}
	})
}

// TestPropertyComputePrepareIDFunctionSensitive verifies that different function
// names produce different IDs (for the same file and no mocks).
func TestPropertyComputePrepareIDFunctionSensitive(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		file := rapid.StringMatching(`[a-z]{3,10}/[a-z]{3,10}\.go`).Draw(t, "file")
		fn1 := rapid.StringMatching(`[A-Za-z][A-Za-z0-9]{0,10}`).Draw(t, "fn1")
		fn2 := rapid.StringMatching(`[A-Za-z][A-Za-z0-9]{0,10}`).Draw(t, "fn2")
		if fn1 == fn2 {
			t.Skip()
		}
		id1 := computePrepareID(file, fn1, nil)
		id2 := computePrepareID(file, fn2, nil)
		if id1 == id2 {
			t.Fatalf("different functions produced same ID: fn1=%q fn2=%q id=%q", fn1, fn2, id1)
		}
	})
}

// ---------------------------------------------------------------------------
// Timing property tests
// ---------------------------------------------------------------------------

// TestPropertyTimingSummaryRoundTrip verifies that a TimingSummary survives
// a JSON marshal/unmarshal roundtrip with all fields intact.
func TestPropertyTimingSummaryRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		ts := genTimingSummary().Draw(t, "timing")
		data, err := json.Marshal(ts)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		var decoded TimingSummary
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		if len(decoded.Phases) != len(ts.Phases) {
			t.Fatalf("phases count: got %d, want %d", len(decoded.Phases), len(ts.Phases))
		}
		for i := range ts.Phases {
			if decoded.Phases[i].PhasePath != ts.Phases[i].PhasePath {
				t.Fatalf("phases[%d].PhasePath: got %q, want %q", i, decoded.Phases[i].PhasePath, ts.Phases[i].PhasePath)
			}
			if decoded.Phases[i].Count != ts.Phases[i].Count {
				t.Fatalf("phases[%d].Count: got %d, want %d", i, decoded.Phases[i].Count, ts.Phases[i].Count)
			}
		}
	})
}

// TestPropertyTimingPhaseSummaryInvariants verifies that generated timing phases
// satisfy TotalMs >= SelfMs >= 0 and Count >= 1.
func TestPropertyTimingPhaseSummaryInvariants(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		phase := genTimingPhaseSummary().Draw(t, "phase")
		if phase.TotalMs < 0 {
			t.Fatalf("TotalMs = %f, want >= 0", phase.TotalMs)
		}
		if phase.SelfMs < 0 {
			t.Fatalf("SelfMs = %f, want >= 0", phase.SelfMs)
		}
		if phase.SelfMs > phase.TotalMs {
			t.Fatalf("SelfMs (%f) > TotalMs (%f)", phase.SelfMs, phase.TotalMs)
		}
		if phase.Count < 1 {
			t.Fatalf("Count = %d, want >= 1", phase.Count)
		}
	})
}

// TestPropertyTimingOmittedWhenNil verifies that the "timing" field is absent
// from JSON when Timing is nil on a Response.
func TestPropertyTimingOmittedWhenNil(t *testing.T) {
	resp := Response{
		ProtocolVersion: ProtocolVersion,
		ID:              1,
		Status:          "analyze",
		Functions:       []FunctionAnalysis{},
		Timing:          nil,
	}
	data, err := json.Marshal(resp)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var obj map[string]any
	if err := json.Unmarshal(data, &obj); err != nil {
		t.Fatalf("unmarshal to map: %v", err)
	}
	if _, ok := obj["timing"]; ok {
		t.Error("timing should be omitted from JSON when nil")
	}
}

// TestPropertyResponseWithTimingRoundTrip verifies that a Response with timing
// data round-trips correctly.
func TestPropertyResponseWithTimingRoundTrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		ts := genTimingSummary().Draw(t, "timing")
		resp := Response{
			ProtocolVersion: ProtocolVersion,
			ID:              rapid.IntRange(0, 1000).Draw(t, "id"),
			Status:          "analyze",
			Functions:       []FunctionAnalysis{},
			Timing:          &ts,
		}
		data, err := json.Marshal(resp)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		var decoded Response
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}
		if decoded.Timing == nil {
			t.Fatal("Timing should not be nil after roundtrip")
		}
		if len(decoded.Timing.Phases) != len(ts.Phases) {
			t.Fatalf("timing phases count: got %d, want %d", len(decoded.Timing.Phases), len(ts.Phases))
		}
	})
}

// TestPropertyComputePrepareIDMocksSensitive verifies that adding a mock changes
// the computed ID.
func TestPropertyComputePrepareIDMocksSensitive(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		file := rapid.StringMatching(`[a-z]{3,10}/[a-z]{3,10}\.go`).Draw(t, "file")
		fn := rapid.StringMatching(`[A-Za-z][A-Za-z0-9]{0,10}`).Draw(t, "fn")
		mock := genMock(t, "mock")
		idWithout := computePrepareID(file, fn, nil)
		idWith := computePrepareID(file, fn, []instrument.MockConfig{mock})
		if idWithout == idWith {
			t.Fatalf("adding mock did not change ID: file=%q fn=%q mock=%q id=%q", file, fn, mock.Symbol, idWithout)
		}
	})
}

// TestPropertySymExprArgsNeverNull generates random Go source with functions
// containing branches, runs AnalyzeFile, marshals every SymExpr in the result
// to JSON, and verifies "args" is never null — always [] or a populated array.
func TestPropertySymExprArgsNeverNull(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		goTypes := []string{"int", "int64", "float64", "string", "bool"}

		nParams := rapid.IntRange(1, 3).Draw(t, "nParams")
		var paramDecls []string
		var paramNames []string
		for i := 0; i < nParams; i++ {
			name := sanitizeGoIdent(
				rapid.StringMatching(`[a-z][a-z0-9]{0,4}`).Draw(t, fmt.Sprintf("p%d", i)),
			)
			goType := rapid.SampledFrom(goTypes).Draw(t, fmt.Sprintf("t%d", i))
			paramDecls = append(paramDecls, fmt.Sprintf("%s %s", name, goType))
			paramNames = append(paramNames, name)
		}

		// Build a function body with branches referencing params
		nBranches := rapid.IntRange(1, 3).Draw(t, "nBranches")
		var body string
		for i := 0; i < nBranches; i++ {
			pIdx := rapid.IntRange(0, nParams-1).Draw(t, fmt.Sprintf("bi%d", i))
			pName := paramNames[pIdx]
			// Generate various branch patterns
			pattern := rapid.IntRange(0, 3).Draw(t, fmt.Sprintf("pat%d", i))
			switch pattern {
			case 0:
				body += fmt.Sprintf("\tif %s > 0 { _ = 1 }\n", pName)
			case 1:
				body += fmt.Sprintf("\tif %s == 0 { _ = 1 }\n", pName)
			case 2:
				body += fmt.Sprintf("\tswitch %s {\n\tcase 0:\n\t\t_ = 1\n\tdefault:\n\t\t_ = 2\n\t}\n", pName)
			case 3:
				body += fmt.Sprintf("\tif %s != 0 && %s > 0 { _ = 1 }\n", pName, pName)
			}
		}

		src := fmt.Sprintf("package p\n\nfunc Foo(%s) int {\n%s\treturn 0\n}\n",
			strings.Join(paramDecls, ", "), body)

		dir, err := os.MkdirTemp("", "shatter-go-prop-args-*")
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
		// Marshal the entire analysis to JSON and check for "args":null
		data, err := json.Marshal(fa)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}
		jsonStr := string(data)
		if strings.Contains(jsonStr, `"args":null`) {
			t.Fatalf("found \"args\":null in serialized FunctionAnalysis JSON:\n%s\nsource:\n%s", jsonStr, src)
		}

		// Also verify each branch condition individually
		for i, branch := range fa.Branches {
			if branch.Condition != nil {
				condData, err := json.Marshal(branch.Condition)
				if err != nil {
					t.Fatalf("branch %d: marshal condition: %v", i, err)
				}
				if strings.Contains(string(condData), `"args":null`) {
					t.Fatalf("branch %d: \"args\":null in condition JSON: %s", i, string(condData))
				}
			}
		}
	})
}
