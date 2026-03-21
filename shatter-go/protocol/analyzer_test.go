package protocol

import (
	"go/token"
	"path/filepath"
	"runtime"
	"testing"
)

func testdataPath(name string) string {
	_, file, _, _ := runtime.Caller(0)
	return filepath.Join(filepath.Dir(file), "testdata", name)
}

// --- Basic type extraction ---

func TestAnalyzeAddReturnsIntParams(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("basic.go"), "Add")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	fn := results[0]
	if fn.Name != "Add" {
		t.Errorf("name = %q, want Add", fn.Name)
	}
	if len(fn.Params) != 2 {
		t.Fatalf("params len = %d, want 2", len(fn.Params))
	}
	for _, p := range fn.Params {
		if p.Type.Kind != "int" {
			t.Errorf("param %s type = %q, want int", p.Name, p.Type.Kind)
		}
	}
	if fn.Params[0].Name != "a" || fn.Params[1].Name != "b" {
		t.Errorf("param names = [%s, %s], want [a, b]", fn.Params[0].Name, fn.Params[1].Name)
	}
	if fn.ReturnType.Kind != "int" {
		t.Errorf("return type = %q, want int", fn.ReturnType.Kind)
	}
}

func TestAnalyzeGreetReturnsStringParams(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("basic.go"), "Greet")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Params) != 1 {
		t.Fatalf("params len = %d, want 1", len(fn.Params))
	}
	if fn.Params[0].Type.Kind != "str" {
		t.Errorf("param type = %q, want str", fn.Params[0].Type.Kind)
	}
	if fn.ReturnType.Kind != "str" {
		t.Errorf("return type = %q, want str", fn.ReturnType.Kind)
	}
}

func TestAnalyzeMaxReturnsFloatParams(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("basic.go"), "Max")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Params) != 2 {
		t.Fatalf("params len = %d, want 2", len(fn.Params))
	}
	for _, p := range fn.Params {
		if p.Type.Kind != "float" {
			t.Errorf("param %s type = %q, want float", p.Name, p.Type.Kind)
		}
	}
}

func TestAnalyzeIsEvenReturnsBoolType(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("basic.go"), "IsEven")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if fn.ReturnType.Kind != "bool" {
		t.Errorf("return type = %q, want bool", fn.ReturnType.Kind)
	}
}

// --- Struct types ---

func TestAnalyzeDistanceAcceptsStruct(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("types.go"), "Distance")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Params) != 1 {
		t.Fatalf("params len = %d, want 1", len(fn.Params))
	}
	p := fn.Params[0]
	if p.Type.Kind != "object" {
		t.Fatalf("param type kind = %q, want object", p.Type.Kind)
	}
	if len(p.Type.Fields) != 2 {
		t.Fatalf("fields len = %d, want 2", len(p.Type.Fields))
	}
	if p.Type.Fields[0].Name != "X" || p.Type.Fields[0].Type.Kind != "float" {
		t.Errorf("field[0] = %+v, want {X, float}", p.Type.Fields[0])
	}
}

func TestAnalyzeProcessOrderHasStructParam(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("types.go"), "ProcessOrder")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	p := fn.Params[0]
	if p.Type.Kind != "object" {
		t.Fatalf("param type kind = %q, want object", p.Type.Kind)
	}
	fieldNames := make(map[string]string)
	for _, f := range p.Type.Fields {
		fieldNames[f.Name] = f.Type.Kind
	}
	expected := map[string]string{
		"ID": "int", "Items": "array", "Priority": "str", "Total": "float",
	}
	for name, kind := range expected {
		if got, ok := fieldNames[name]; !ok {
			t.Errorf("missing field %q", name)
		} else if got != kind {
			t.Errorf("field %q kind = %q, want %q", name, got, kind)
		}
	}
}

// --- Slice types ---

func TestAnalyzeScaleSliceAcceptsSlice(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("types.go"), "ScaleSlice")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Params) != 2 {
		t.Fatalf("params len = %d, want 2", len(fn.Params))
	}
	sliceParam := fn.Params[0]
	if sliceParam.Type.Kind != "array" {
		t.Fatalf("param type kind = %q, want array", sliceParam.Type.Kind)
	}
	if sliceParam.Type.Element == nil || sliceParam.Type.Element.Kind != "float" {
		t.Errorf("element kind = %v, want float", sliceParam.Type.Element)
	}
}

// --- Map types ---

func TestAnalyzeLookupMapAcceptsMap(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("types.go"), "LookupMap")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if fn.Params[0].Type.Kind != "object" {
		t.Errorf("map param type = %q, want object", fn.Params[0].Type.Kind)
	}
}

// --- Pointer types ---

func TestAnalyzeProcessPointerAcceptsPointer(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("types.go"), "ProcessPointer")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	p := fn.Params[0]
	if p.Type.Kind != "nullable" {
		t.Fatalf("param type kind = %q, want nullable", p.Type.Kind)
	}
	if p.Type.Inner == nil || p.Type.Inner.Kind != "object" {
		t.Errorf("inner kind = %v, want object", p.Type.Inner)
	}
}

// --- Multiple return values ---

func TestAnalyzeLookupMapHasMultipleReturns(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("types.go"), "LookupMap")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if fn.ReturnType.Kind != "object" {
		t.Fatalf("return type kind = %q, want object (tuple)", fn.ReturnType.Kind)
	}
	if len(fn.ReturnType.Fields) != 2 {
		t.Fatalf("return fields len = %d, want 2", len(fn.ReturnType.Fields))
	}
}

// --- Interface types ---

func TestAnalyzeFormatValueAcceptsInterface(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("interfaces.go"), "FormatValue")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if fn.Params[0].Type.Kind != "unknown" {
		t.Errorf("interface param type = %q, want unknown", fn.Params[0].Type.Kind)
	}
}

func TestAnalyzeFormatAnyAcceptsEmptyInterface(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("interfaces.go"), "FormatAny")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if fn.Params[0].Type.Kind != "unknown" {
		t.Errorf("interface{} param type = %q, want unknown", fn.Params[0].Type.Kind)
	}
}

// --- Branch extraction ---

func TestAnalyzeGreetExtractsIfBranch(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("basic.go"), "Greet")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Branches) != 1 {
		t.Fatalf("branches len = %d, want 1", len(fn.Branches))
	}
	br := fn.Branches[0]
	if br.BranchType != "if" {
		t.Errorf("branch_type = %q, want if", br.BranchType)
	}
	if br.ConditionText != `name == ""` {
		t.Errorf("condition_text = %q, want name == \"\"", br.ConditionText)
	}
}

func TestAnalyzeClassifyExtractsMultipleBranches(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("basic.go"), "Classify")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Branches) < 2 {
		t.Fatalf("branches len = %d, want >= 2", len(fn.Branches))
	}
}

func TestAnalyzeSwitchOnStringExtractsCaseBranches(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("switches.go"), "SwitchOnString")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Branches) != 3 {
		t.Fatalf("branches len = %d, want 3", len(fn.Branches))
	}
	for _, br := range fn.Branches {
		if br.BranchType != "switch" {
			t.Errorf("branch_type = %q, want switch", br.BranchType)
		}
	}
}

func TestAnalyzeForLoopExtractsForBranch(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("switches.go"), "ForLoop")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Branches) != 1 {
		t.Fatalf("branches len = %d, want 1", len(fn.Branches))
	}
	if fn.Branches[0].BranchType != "for" {
		t.Errorf("branch_type = %q, want for", fn.Branches[0].BranchType)
	}
}

func TestAnalyzeLogicalOpsExtractsBranches(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("switches.go"), "LogicalOps")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Branches) != 2 {
		t.Fatalf("branches len = %d, want 2", len(fn.Branches))
	}
}

func TestAnalyzeScaleSliceExtractsRangeBranch(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("types.go"), "ScaleSlice")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Branches) != 1 {
		t.Fatalf("branches len = %d, want 1", len(fn.Branches))
	}
	if fn.Branches[0].BranchType != "for" {
		t.Errorf("branch_type = %q, want for", fn.Branches[0].BranchType)
	}
}

// --- Select statement branch extraction ---

func TestAnalyzeSelectBranches(t *testing.T) {
	tests := []struct {
		name       string
		funcName   string
		wantCount  int
		wantCases  []struct {
			conditionText string
			branchType    string
		}
	}{
		{
			name:      "with default has three branches",
			funcName:  "SelectExample",
			wantCount: 3,
			wantCases: []struct {
				conditionText string
				branchType    string
			}{
				{conditionText: "v := <-ch1", branchType: "select"},
				{conditionText: `ch2 <- "hello"`, branchType: "select"},
				{conditionText: "default", branchType: "select"},
			},
		},
		{
			name:      "without default has two branches",
			funcName:  "SelectNoDefault",
			wantCount: 2,
			wantCases: []struct {
				conditionText string
				branchType    string
			}{
				{conditionText: "v := <-ch1", branchType: "select"},
				{conditionText: "v := <-ch2", branchType: "select"},
			},
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			results, err := AnalyzeFile(testdataPath("select.go"), tc.funcName)
			if err != nil {
				t.Fatalf("AnalyzeFile: %v", err)
			}
			fn := results[0]
			if len(fn.Branches) != tc.wantCount {
				t.Fatalf("branches len = %d, want %d", len(fn.Branches), tc.wantCount)
			}
			for i, want := range tc.wantCases {
				t.Run(want.conditionText, func(t *testing.T) {
					if fn.Branches[i].BranchType != want.branchType {
						t.Errorf("branch_type = %q, want %q", fn.Branches[i].BranchType, want.branchType)
					}
					if fn.Branches[i].ConditionText != want.conditionText {
						t.Errorf("condition_text = %q, want %q", fn.Branches[i].ConditionText, want.conditionText)
					}
				})
			}
		})
	}
}

// --- Symbolic expression construction ---

func TestAnalyzeGreetBranchHasSymExpr(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("basic.go"), "Greet")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	br := results[0].Branches[0]
	if br.Condition == nil {
		t.Fatal("condition is nil")
	}
	if br.Condition.Kind != "bin_op" {
		t.Fatalf("condition kind = %q, want bin_op", br.Condition.Kind)
	}
	if br.Condition.Op != "eq" {
		t.Errorf("condition op = %q, want eq", br.Condition.Op)
	}
	if br.Condition.Left == nil || br.Condition.Left.Kind != "param" {
		t.Errorf("left = %+v, want param", br.Condition.Left)
	}
	if br.Condition.Left.Name != "name" {
		t.Errorf("left.name = %q, want name", br.Condition.Left.Name)
	}
	if br.Condition.Right == nil || br.Condition.Right.Kind != "const" {
		t.Errorf("right = %+v, want const", br.Condition.Right)
	}
}

func TestAnalyzeProcessOrderBranchReferencesStructField(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("types.go"), "ProcessOrder")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Branches) < 1 {
		t.Fatal("no branches")
	}
	br := fn.Branches[0]
	if br.Condition == nil || br.Condition.Left == nil {
		t.Fatal("expected condition with left operand")
	}
	left := br.Condition.Left
	if left.Kind != "param" {
		t.Fatalf("left kind = %q, want param", left.Kind)
	}
	if left.Name != "order" {
		t.Errorf("left.name = %q, want order", left.Name)
	}
	if len(left.Path) != 1 || left.Path[0] != "Priority" {
		t.Errorf("left.path = %v, want [Priority]", left.Path)
	}
}

func TestAnalyzeSwitchCaseHasEqSymExpr(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("switches.go"), "SwitchOnString")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	br := results[0].Branches[0]
	if br.Condition == nil {
		t.Fatal("condition is nil")
	}
	if br.Condition.Kind != "bin_op" || br.Condition.Op != "eq" {
		t.Errorf("condition = %+v, want bin_op eq", br.Condition)
	}
}

func TestTokenToOpBitwise(t *testing.T) {
	tests := []struct {
		tok token.Token
		op  string
	}{
		{token.AND, "bitwise_and"},
		{token.OR, "bitwise_or"},
		{token.XOR, "bitwise_xor"},
		{token.SHL, "shl"},
		{token.SHR, "shr"},
		{token.AND_NOT, "bit_clear"},
	}
	for _, tc := range tests {
		got := tokenToOp(tc.tok)
		if got != tc.op {
			t.Errorf("tokenToOp(%v) = %q, want %q", tc.tok, got, tc.op)
		}
	}
}

// --- Dependency detection ---

func TestAnalyzeFormatNameDetectsDependencies(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("deps.go"), "FormatName")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Dependencies) == 0 {
		t.Fatal("expected dependencies, got none")
	}
	symbols := make(map[string]bool)
	for _, d := range fn.Dependencies {
		symbols[d.Symbol] = true
		if d.Kind != "function_call" {
			t.Errorf("dep %q kind = %q, want function_call", d.Symbol, d.Kind)
		}
		if d.SourceModule == "" {
			t.Errorf("dep %q has empty source_module", d.Symbol)
		}
		if len(d.CallSites) == 0 {
			t.Errorf("dep %q has no call sites", d.Symbol)
		}
	}
	if !symbols["strings.TrimSpace"] {
		t.Errorf("missing dependency strings.TrimSpace, got %v", symbols)
	}
	if !symbols["fmt.Sprintf"] {
		t.Errorf("missing dependency fmt.Sprintf, got %v", symbols)
	}
}

func TestAnalyzeFormatNameTrimSpaceHasMultipleCallSites(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("deps.go"), "FormatName")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	for _, d := range results[0].Dependencies {
		if d.Symbol == "strings.TrimSpace" {
			if len(d.CallSites) != 2 {
				t.Errorf("TrimSpace call_sites len = %d, want 2", len(d.CallSites))
			}
			return
		}
	}
	t.Fatal("strings.TrimSpace not found in dependencies")
}

// --- Line numbers ---

func TestAnalyzeAddHasCorrectLineNumbers(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("basic.go"), "Add")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if fn.StartLine < 1 {
		t.Errorf("start_line = %d, want >= 1", fn.StartLine)
	}
	if fn.EndLine <= fn.StartLine {
		t.Errorf("end_line %d should be > start_line %d", fn.EndLine, fn.StartLine)
	}
}

// --- File-level analysis ---

func TestAnalyzeFileWithoutFunctionReturnsAll(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("basic.go"), "")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	// basic.go has 6 functions: Add, Greet, Classify, Max, IsEven, noExport
	if len(results) != 6 {
		names := make([]string, len(results))
		for i, r := range results {
			names[i] = r.Name
		}
		t.Fatalf("got %d results %v, want 6", len(results), names)
	}
}

// --- Error handling ---

func TestAnalyzeNonexistentFileFails(t *testing.T) {
	_, err := AnalyzeFile("/nonexistent/file.go", "")
	if err == nil {
		t.Fatal("expected error for nonexistent file")
	}
}

func TestAnalyzeMissingFunctionFails(t *testing.T) {
	_, err := AnalyzeFile(testdataPath("basic.go"), "NonexistentFunc")
	if err == nil {
		t.Fatal("expected error for missing function")
	}
}

// --- Branch IDs are sequential ---

func TestBranchIDsAreSequential(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("basic.go"), "Classify")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	for i, br := range fn.Branches {
		if br.ID != i {
			t.Errorf("branch[%d].id = %d, want %d", i, br.ID, i)
		}
	}
}

// --- Empty params/branches produce empty slices not nil ---

func TestAnalyzeAddHasEmptyBranchesSlice(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("basic.go"), "Add")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if fn.Branches == nil {
		t.Error("branches should be empty slice, not nil")
	}
	if len(fn.Branches) != 0 {
		t.Errorf("branches len = %d, want 0", len(fn.Branches))
	}
}

func TestAnalyzeNoExportHasEmptyParamsSlice(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("basic.go"), "noExport")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if fn.Params == nil {
		t.Error("params should be empty slice, not nil")
	}
}

// --- Opaque type detection ---

func TestAnalyzeOpaqueTypes(t *testing.T) {
	tests := []struct {
		funcName  string
		paramName string
		wantKind  string
		wantLabel string
	}{
		{"AcceptsChanInt", "ch", "opaque", "chan int"},
		{"AcceptsChanString", "ch", "opaque", "chan string"},
		{"AcceptsNetConn", "conn", "opaque", "net.Conn"},
		{"AcceptsOsFile", "f", "opaque", "os.File"},
		{"AcceptsIOReader", "r", "opaque", "io.Reader"},
		{"AcceptsIOWriter", "w", "opaque", "io.Writer"},
		{"AcceptsSqlDB", "db", "opaque", "sql.DB"},
		{"AcceptsSqlTx", "tx", "opaque", "sql.Tx"},
		{"AcceptsResponseWriter", "w", "opaque", "http.ResponseWriter"},
		{"AcceptsNetListener", "ln", "opaque", "net.Listener"},
	}

	for _, tc := range tests {
		t.Run(tc.funcName, func(t *testing.T) {
			results, err := AnalyzeFile(testdataPath("opaque.go"), tc.funcName)
			if err != nil {
				t.Fatalf("AnalyzeFile: %v", err)
			}
			fn := results[0]
			if len(fn.Params) < 1 {
				t.Fatalf("params len = %d, want >= 1", len(fn.Params))
			}
			p := fn.Params[0]
			if p.Name != tc.paramName {
				t.Errorf("param name = %q, want %q", p.Name, tc.paramName)
			}
			if p.Type.Kind != tc.wantKind {
				t.Errorf("param type kind = %q, want %q", p.Type.Kind, tc.wantKind)
			}
			if p.Type.Label != tc.wantLabel {
				t.Errorf("param type label = %q, want %q", p.Type.Label, tc.wantLabel)
			}
		})
	}
}

func TestAnalyzePlainInterfaceStillReturnsUnknown(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("opaque.go"), "AcceptsPlainInterface")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if fn.Params[0].Type.Kind != "unknown" {
		t.Errorf("plain interface type = %q, want unknown", fn.Params[0].Type.Kind)
	}
}

func TestAnalyzeSelectExampleChannelParamsAreOpaque(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("select.go"), "SelectExample")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Params) != 2 {
		t.Fatalf("params len = %d, want 2", len(fn.Params))
	}
	if fn.Params[0].Type.Kind != "opaque" {
		t.Errorf("ch1 type kind = %q, want opaque", fn.Params[0].Type.Kind)
	}
	if fn.Params[0].Type.Label != "chan int" {
		t.Errorf("ch1 type label = %q, want %q", fn.Params[0].Type.Label, "chan int")
	}
	if fn.Params[1].Type.Kind != "opaque" {
		t.Errorf("ch2 type kind = %q, want opaque", fn.Params[1].Type.Kind)
	}
	if fn.Params[1].Type.Label != "chan string" {
		t.Errorf("ch2 type label = %q, want %q", fn.Params[1].Type.Label, "chan string")
	}
}

// --- Literal extraction ---

func TestExtractLiterals_StringsFromConditions(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("literals.go"), "ClassifyPriority")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	fn := results[0]
	strs := filterLiterals(fn.Literals, "str")
	for _, want := range []string{"express", "economy", "standard"} {
		if !containsLitValue(strs, want) {
			t.Errorf("missing string literal %q in %v", want, strs)
		}
	}
}

func TestExtractLiterals_IntsFromSwitch(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("literals.go"), "GradeScore")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	ints := filterLiterals(fn.Literals, "int")
	for _, want := range []int64{90, 70, 50} {
		if !containsLitValue(ints, want) {
			t.Errorf("missing int literal %d in %v", want, ints)
		}
	}
}

func TestExtractLiterals_RegexpMustCompile(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("literals.go"), "ValidateZip")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	regexes := filterLiterals(fn.Literals, "regex")
	if len(regexes) != 1 {
		t.Fatalf("got %d regex literals, want 1", len(regexes))
	}
	if regexes[0].Pattern != `^\d{5}$` {
		t.Errorf("regex pattern = %q, want %q", regexes[0].Pattern, `^\d{5}$`)
	}
}

func TestExtractLiterals_NoBodyLiterals(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("literals.go"), "NoLiterals")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	// NoLiterals has no body literals, but file-level consts (MaxRetries=3,
	// Threshold=0.75, Prefix="v1") are now included for all functions.
	if len(fn.Literals) != 3 {
		t.Errorf("expected 3 file-level literals, got %d", len(fn.Literals))
	}
}

func TestExtractLiterals_Deduplication(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("literals.go"), "WithDuplicates")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	okCount := 0
	for _, lit := range fn.Literals {
		if lit.Type == "str" && lit.Value == "ok" {
			okCount++
		}
	}
	if okCount != 1 {
		t.Errorf("expected 1 'ok' literal, got %d", okCount)
	}
}

func TestExtractLiterals_FileConstants(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("literals.go"), "UseFileConsts")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	ints := filterLiterals(fn.Literals, "int")
	if !containsLitValue(ints, int64(3)) {
		t.Error("expected file-level const MaxRetries=3 in literals")
	}
	strs := filterLiterals(fn.Literals, "str")
	if !containsLitValue(strs, "v1") {
		t.Error("expected file-level const Prefix=\"v1\" in literals")
	}
	floats := filterLiterals(fn.Literals, "float")
	if !containsLitValue(floats, 0.75) {
		t.Error("expected file-level const Threshold=0.75 in literals")
	}
}

func TestExtractLiterals_MapKeyAccess(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("literals.go"), "CheckMapKey")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	strs := filterLiterals(fn.Literals, "str")
	if !containsLitValue(strs, "status") {
		t.Error("expected map key \"status\" in literals")
	}
}

// Test helpers for literal assertions
func filterLiterals(lits []LiteralValue, typ string) []LiteralValue {
	var out []LiteralValue
	for _, l := range lits {
		if l.Type == typ {
			out = append(out, l)
		}
	}
	return out
}

func containsLitValue(lits []LiteralValue, val any) bool {
	for _, l := range lits {
		if l.Value == val {
			return true
		}
		if l.Pattern == val {
			return true
		}
	}
	return false
}

// --- Static opacity heuristics ---

func TestStaticOpacityHeuristics(t *testing.T) {
	tests := []struct {
		funcName   string
		wantKind   string
		wantReason string
	}{
		// InternalConn: all fields unexported, no factory → no_constructor
		{"UseInternalConn", "opaque", "no_constructor"},
	}
	for _, tc := range tests {
		t.Run(tc.funcName, func(t *testing.T) {
			fns, err := AnalyzeFile(testdataPath("static_opaque.go"), tc.funcName)
			if err != nil {
				t.Fatalf("AnalyzeFile: %v", err)
			}
			if len(fns) == 0 {
				t.Fatal("no functions returned")
			}
			p := fns[0].Params[0]
			if p.Type.Kind != tc.wantKind {
				t.Errorf("kind = %q, want %q", p.Type.Kind, tc.wantKind)
			}
			if p.Type.StaticOpacity != tc.wantReason {
				t.Errorf("static_opacity = %q, want %q", p.Type.StaticOpacity, tc.wantReason)
			}
		})
	}
}
