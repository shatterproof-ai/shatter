package protocol

import (
	"encoding/json"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
)

func testdataPath(name string) string {
	_, file, _, _ := runtime.Caller(0)
	return filepath.Join(filepath.Dir(file), "testdata", name)
}

func firstFuncDecl(t *testing.T, file *ast.File) *ast.FuncDecl {
	t.Helper()
	for _, decl := range file.Decls {
		if fn, ok := decl.(*ast.FuncDecl); ok {
			return fn
		}
	}
	t.Fatal("no function declaration found")
	return nil
}

func assertStringSlice(t *testing.T, got []string, want []string) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("len(got) = %d, want %d; got=%v", len(got), len(want), got)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("got[%d] = %q, want %q; got=%v", i, got[i], want[i], got)
		}
	}
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

func TestStringLiteralCandidatesByParamSwitchAndComparison(t *testing.T) {
	src := `package p

type Config struct {
	Mode string
}

func choose(cfg Config, state string) int {
	switch cfg.Mode {
	case "fixed":
		return 1
	case "random":
		return 2
	}
	if state == "ready" {
		return 3
	}
	if "blocked" != state {
		return 4
	}
	return 0
}`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "literal_candidates.go", src, 0)
	if err != nil {
		t.Fatalf("ParseFile: %v", err)
	}
	fn := firstFuncDecl(t, file)
	got := stringLiteralCandidatesByParam(fn, nil, []ParamInfo{
		{Name: "cfg", Type: TypeInfo{Kind: "object"}},
		{Name: "state", Type: TypeInfo{Kind: "str"}},
	})
	assertStringSlice(t, got["cfg.Mode"], []string{"fixed", "random"})
	assertStringSlice(t, got["state"], []string{"ready", "blocked"})
}

func TestStringLiteralCandidatesByParamIndexedStringSlice(t *testing.T) {
	src := `package p

func choose(args []string) int {
	switch args[0] {
	case "list":
		return 1
	case "create":
		return 2
	case "delete":
		return 3
	}
	return 0
}`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "indexed_literal_candidates.go", src, 0)
	if err != nil {
		t.Fatalf("ParseFile: %v", err)
	}
	fn := firstFuncDecl(t, file)
	got := stringLiteralCandidatesByParam(fn, nil, []ParamInfo{
		{
			Name: "args",
			Type: TypeInfo{Kind: "array", Element: &TypeInfo{Kind: "str"}},
		},
	})
	assertStringSlice(t, got["args"], []string{"list", "create", "delete"})
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
	// str-23mc: non-synthesizable interface params are opaque so the core
	// does not send raw JSON scalars the wrapper can't unmarshal.
	if fn.Params[0].Type.Kind != "opaque" {
		t.Errorf("interface param type = %q, want opaque", fn.Params[0].Type.Kind)
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
	if fn.Params[0].Type.Label != "interface" {
		t.Errorf("interface{} param label = %q, want interface", fn.Params[0].Type.Label)
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
	// 3 value cases + default = 4 branch obligations (str-qo1.11: each
	// CaseClause is an independent obligation, including the default
	// catch-all).
	const wantBranches = 4
	if len(fn.Branches) != wantBranches {
		t.Fatalf("branches len = %d, want %d", len(fn.Branches), wantBranches)
	}
	for _, br := range fn.Branches {
		if br.BranchType != "switch" {
			t.Errorf("branch_type = %q, want switch", br.BranchType)
		}
	}
}

// TestAnalyzeMultiLiteralCaseEmitsDisjunction is the str-5jen analyzer
// regression: a `case 2, 3:` clause must surface a disjunctive symbolic
// Condition (op="or", both literals as constant operands) and a
// disjunctive ConditionText. Pre-fix only the first literal reached the
// SymExpr, leaving the second literal's path symbolically unreachable.
func TestAnalyzeMultiLiteralCaseEmitsDisjunction(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("switches.go"), "MultiLiteralSwitch")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	const wantBranches = 3 // case 1, case 2|3, default
	if len(fn.Branches) != wantBranches {
		t.Fatalf("branches len = %d, want %d", len(fn.Branches), wantBranches)
	}
	multi := fn.Branches[1]
	if got, want := multi.ConditionText, "x == 2 || x == 3"; got != want {
		t.Errorf("ConditionText = %q, want %q", got, want)
	}
	if multi.Condition == nil {
		t.Fatalf("Condition is nil for multi-literal clause")
	}
	if multi.Condition.Op != "or" {
		t.Errorf("Condition.Op = %q, want %q", multi.Condition.Op, "or")
	}
	// Both literals must appear as `const` operands somewhere in the
	// disjunction. Use a recursive walk so the test does not depend on
	// left/right associativity of the chained `or`.
	wantLiterals := map[int64]bool{2: false, 3: false}
	var walk func(*SymExpr)
	walk = func(e *SymExpr) {
		if e == nil {
			return
		}
		if e.Kind == "const" {
			if v, ok := e.Value.(int64); ok {
				if _, want := wantLiterals[v]; want {
					wantLiterals[v] = true
				}
			}
		}
		walk(e.Left)
		walk(e.Right)
	}
	walk(multi.Condition)
	for lit, seen := range wantLiterals {
		if !seen {
			t.Errorf("literal %d missing from multi-literal disjunction: %+v", lit, multi.Condition)
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
		name      string
		funcName  string
		wantCount int
		wantCases []struct {
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

// TestAnalyzeCategorizeIteInBranchCondition is the str-1hlk.17.3 integration
// test.  It feeds examples/go/05-conditional-merge.go::Categorize to the
// analyzer and asserts that the second branch condition contains an ite
// SymExpr — the result of threading the data-flow map through
// extractBranches.
//
// Categorize assigns `label` (1 or -1) conditionally across an if/else, then
// tests `label > 0`.  After the flow-map walk, label's symbolic value is
// ite{condition: x>0, then_expr: 1, else_expr: -1}.  The second branch
// condition therefore resolves to  bin_op{gt, ite{...}, const{0}}.
func TestAnalyzeCategorizeIteInBranchCondition(t *testing.T) {
	_, thisFile, _, _ := runtime.Caller(0)
	// Navigate from shatter-go/protocol/ to repo root (two levels up)
	repoRoot := filepath.Clean(filepath.Join(filepath.Dir(thisFile), "..", ".."))
	p := filepath.Join(repoRoot, "examples", "go", "05-conditional-merge.go")
	if _, err := os.Stat(p); err != nil {
		t.Skipf("example file not found at %s: %v", p, err)
	}

	results, err := AnalyzeFile(p, "Categorize")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	fn := results[0]
	if len(fn.Branches) < 2 {
		t.Fatalf("Categorize has %d branches, want >= 2 (if x>0 and if label>0)", len(fn.Branches))
	}

	// Branch 0: plain parameter comparison — x > 0.
	br0 := fn.Branches[0]
	if br0.Condition == nil {
		t.Fatal("branch 0 condition is nil")
	}
	if br0.Condition.Kind != "bin_op" || br0.Condition.Op != "gt" {
		t.Errorf("branch 0: want bin_op/gt, got kind=%q op=%q", br0.Condition.Kind, br0.Condition.Op)
	}
	if br0.Condition.Left == nil || br0.Condition.Left.Kind != "param" {
		t.Errorf("branch 0 left: want param, got %+v", br0.Condition.Left)
	}

	// Branch 1: label > 0 — label resolves to ite(x>0, 1, -1).
	br1 := fn.Branches[1]
	if br1.Condition == nil {
		t.Fatal("branch 1 condition is nil")
	}
	if br1.Condition.Kind != "bin_op" || br1.Condition.Op != "gt" {
		t.Errorf("branch 1: want bin_op/gt, got kind=%q op=%q", br1.Condition.Kind, br1.Condition.Op)
	}
	left1 := br1.Condition.Left
	if left1 == nil {
		t.Fatal("branch 1 left is nil")
	}
	if left1.Kind != "ite" {
		t.Errorf("branch 1 left kind = %q, want ite (label resolves to ite via flow map)", left1.Kind)
	}
	// The ite condition should be x > 0.
	if left1.Condition == nil {
		t.Fatal("ite.condition is nil")
	}
	if left1.Condition.Kind != "bin_op" || left1.Condition.Op != "gt" {
		t.Errorf("ite.condition: want bin_op/gt, got kind=%q op=%q", left1.Condition.Kind, left1.Condition.Op)
	}
	// then_expr = 1 (const int); else_expr = -1 which in Go AST is
	// un_op{neg, const{1}} rather than const{-1}.
	if left1.ThenExpr == nil || left1.ThenExpr.Kind != "const" {
		t.Errorf("ite.then_expr: want const, got %+v", left1.ThenExpr)
	}
	if left1.ElseExpr == nil {
		t.Fatal("ite.else_expr is nil")
	}
	// -1 is represented as un_op{neg, const{1}} at the AST level.
	if left1.ElseExpr.Kind != "un_op" || left1.ElseExpr.Op != "neg" {
		t.Errorf("ite.else_expr: want un_op/neg (representing -1), got kind=%q op=%q",
			left1.ElseExpr.Kind, left1.ElseExpr.Op)
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

func TestBuildUnOpUnsupportedUnaryTokensReturnUnknown(t *testing.T) {
	params := map[string]bool{"x": true}
	tests := []token.Token{token.AND, token.MUL, token.ADD, token.ARROW}

	for _, tok := range tests {
		expr := ast.UnaryExpr{Op: tok, X: &ast.Ident{Name: "x"}}
		got := buildUnOp(&expr, params)
		if got.Kind != "unknown" {
			t.Fatalf("buildUnOp(%v).Kind = %q, want unknown", tok, got.Kind)
		}
	}
}

func TestAnalyzeAddressOfBranchUsesUnknownConstraintOperand(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("unary_exprs.go"), "AddressOfBranch")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}

	br := results[0].Branches[0]
	if br.Condition == nil {
		t.Fatal("condition is nil")
	}
	if br.Condition.Kind != "bin_op" || br.Condition.Op != "ne" {
		t.Fatalf("condition = %+v, want bin_op ne", br.Condition)
	}
	if br.Condition.Left == nil {
		t.Fatal("condition.Left is nil")
	}
	if br.Condition.Left.Kind != "unknown" {
		t.Fatalf("condition.Left.Kind = %q, want unknown", br.Condition.Left.Kind)
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

// TestAnalyzeMultiCaseSwitchCountsClausesIncludingDefault is the str-qo1.11
// regression: a switch with N case clauses + default must report N+1 branches,
// matching the instrumentor's per-CaseClause branch_id assignment in
// shatter-go/instrument/visitor.go transformSwitchStmt. Pre-fix, the
// analyzer enumerated branches per case literal and skipped the default
// clause, so focused exploration of an exhaustive switch reported >100%
// coverage (e.g. "11/10 branches" for a 10-case fixture, because the
// instrumentor recorded an 11th branch_id for the default body).
func TestAnalyzeMultiCaseSwitchCountsClausesIncludingDefault(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("multi_case_switch.go"), "DetectLanguageID")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) == 0 {
		t.Fatal("no functions returned")
	}
	const wantCaseClauses = 10
	const wantBranches = wantCaseClauses + 1 // +1 for the default clause
	got := len(results[0].Branches)
	if got != wantBranches {
		t.Fatalf("len(branches) = %d, want %d (one per CaseClause incl. default)", got, wantBranches)
	}
	// Every emitted branch must be tagged as a switch branch and carry a
	// monotonically-increasing ID matching the instrumentor's clause-keyed
	// numbering (0..N).
	for i, b := range results[0].Branches {
		if b.BranchType != "switch" {
			t.Errorf("branches[%d].BranchType = %q, want \"switch\"", i, b.BranchType)
		}
		if int(b.ID) != i {
			t.Errorf("branches[%d].ID = %d, want %d", i, b.ID, i)
		}
	}
	// The last clause is the default — no concrete case literal to surface,
	// so the analyzer must mark it explicitly so downstream callers can
	// distinguish it from a value case.
	last := results[0].Branches[len(results[0].Branches)-1]
	if last.ConditionText != "default" {
		t.Errorf("last branch ConditionText = %q, want \"default\"", last.ConditionText)
	}
}

// TestAnalyzeFilepathSwitchReportsPathFilepathSourceModule is the
// frontend-side regression for str-qo1.10 (filepath mock purity). The
// analyzer must surface pure helpers like filepath.Ext with
// SourceModule="path/filepath" so the auto-mock classifier in
// shatter-core can recognize them as PureUtility and skip mocking
// (which would otherwise reduce filepath.Ext to "" and hide every
// non-default branch of the switch in DetectServerKey).
func TestAnalyzeFilepathSwitchReportsPathFilepathSourceModule(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("filepath_switch.go"), "DetectServerKey")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) == 0 {
		t.Fatal("no functions returned")
	}
	var extDep *ExternalDependency
	for i := range results[0].Dependencies {
		d := &results[0].Dependencies[i]
		if d.Symbol == "filepath.Ext" {
			extDep = d
			break
		}
	}
	if extDep == nil {
		t.Fatalf("filepath.Ext not found in dependencies; got %+v", results[0].Dependencies)
	}
	if extDep.SourceModule != "path/filepath" {
		t.Errorf("filepath.Ext SourceModule = %q, want %q",
			extDep.SourceModule, "path/filepath")
	}
	if extDep.Kind != "function_call" {
		t.Errorf("filepath.Ext Kind = %q, want function_call", extDep.Kind)
	}
}

// TestAnalyzeJoinAndCheckReportsPurePathFilepathHelpers asserts that the
// other pure path/filepath helpers (Join, Clean, IsAbs, Base, Dir) all
// report SourceModule="path/filepath" so the cross-language auto-mock
// classifier can classify them uniformly as PureUtility (str-qo1.10).
func TestAnalyzeJoinAndCheckReportsPurePathFilepathHelpers(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("filepath_switch.go"), "JoinAndCheck")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) == 0 {
		t.Fatal("no functions returned")
	}
	want := map[string]bool{
		"filepath.Join":  false,
		"filepath.Clean": false,
		"filepath.IsAbs": false,
		"filepath.Base":  false,
		"filepath.Dir":   false,
	}
	for _, d := range results[0].Dependencies {
		if _, expected := want[d.Symbol]; !expected {
			continue
		}
		if d.SourceModule != "path/filepath" {
			t.Errorf("%s SourceModule = %q, want %q",
				d.Symbol, d.SourceModule, "path/filepath")
		}
		want[d.Symbol] = true
	}
	for sym, found := range want {
		if !found {
			t.Errorf("missing dependency %s in JoinAndCheck", sym)
		}
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

// TestAnalyzeCallerCapturesLocalFunctionCalls asserts that intra-package,
// bare-identifier function calls (e.g. Helper(x) where Helper is defined in the
// same package) are reported as dependencies. Without this, the run/analyze
// call graph cannot construct edges for projects whose calls are mostly
// intra-package — see str-ic3b.
func TestAnalyzeCallerCapturesLocalFunctionCalls(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("local_calls.go"), "Caller")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) == 0 {
		t.Fatal("no functions returned")
	}
	fn := results[0]
	symbols := make(map[string]ExternalDependency)
	for _, d := range fn.Dependencies {
		symbols[d.Symbol] = d
	}
	for _, want := range []string{"Helper", "Annotate"} {
		dep, ok := symbols[want]
		if !ok {
			t.Errorf("missing intra-package dependency %q, got %v", want, keysOfDeps(symbols))
			continue
		}
		if dep.Kind != "function_call" {
			t.Errorf("dep %q kind = %q, want function_call", want, dep.Kind)
		}
		if dep.SourceModule == "" {
			t.Errorf("dep %q source_module is empty", want)
		}
		if len(dep.CallSites) == 0 {
			t.Errorf("dep %q has no call sites", want)
		}
	}
}

func keysOfDeps(m map[string]ExternalDependency) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	return out
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

// str-z06h: the Go analyzer surfaces both exported and unexported functions
// with a faithful `Exported` tag. The visibility filter is the CLI's
// concern (`--all` in `shatter-cli/src/commands/scan.rs`); the frontend's
// job is to report what it found, not to gate.
func TestAnalyzeFileReportsExportedAndUnexportedFunctions(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("basic.go"), "")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	byName := make(map[string]bool, len(results))
	for _, r := range results {
		byName[r.Name] = r.Exported
	}
	cases := []struct {
		name     string
		exported bool
	}{
		{"Add", true},
		{"Greet", true},
		{"Classify", true},
		{"Max", true},
		{"IsEven", true},
		{"noExport", false},
	}
	for _, c := range cases {
		got, ok := byName[c.name]
		if !ok {
			t.Errorf("function %q missing from analyzer results", c.name)
			continue
		}
		if got != c.exported {
			t.Errorf("function %q: Exported=%v, want %v", c.name, got, c.exported)
		}
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
		// str-gxjs: io.Reader/io.Writer/http.ResponseWriter are now
		// synthesizable by the planner via the runtime-value registry
		// (strings.NewReader, &bytes.Buffer{}, httptest.NewRecorder()).
		// The analyzer emits Kind="unknown" with TypeName set on the
		// ParamInfo; the opaque categorization moves to a separate
		// assertion in TestAnalyzeSynthesizableStdlibTypes.
		{"AcceptsIOReader", "r", "unknown", "io.Reader"},
		{"AcceptsIOWriter", "w", "unknown", "io.Writer"},
		{"AcceptsSqlDB", "db", "opaque", "sql.DB"},
		{"AcceptsSqlTx", "tx", "opaque", "sql.Tx"},
		{"AcceptsResponseWriter", "w", "unknown", "http.ResponseWriter"},
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

// str-gxjs: io.Reader / io.Writer / io.ReadCloser / http.ResponseWriter /
// *http.Request / context.Context used to be flagged as opaque and the
// function skipped before any planning attempt. The analyzer emits
// Kind="unknown" with the canonical Go-source spelling on
// ParamInfo.TypeName so the planner's runtime-value registry can
// resolve a safe in-memory expression (httptest.NewRecorder() and so on).
// The Rust core's check_executability accepts "unknown" params, so the
// function reaches the explore phase instead of landing in the skipped
// bucket.
func TestAnalyzeSynthesizableStdlibTypes(t *testing.T) {
	cases := []struct {
		funcName     string
		wantTypeName string
	}{
		{"AcceptsIOReader", "io.Reader"},
		{"AcceptsIOWriter", "io.Writer"},
		{"AcceptsResponseWriter", "http.ResponseWriter"},
		{"AcceptsHTTPHandler", "http.Handler"},
		{"AcceptsIOReadCloser", "io.ReadCloser"},
		{"AcceptsContext", "context.Context"},
		{"AcceptsTemplatePointer", "*template.Template"},
	}
	for _, tc := range cases {
		t.Run(tc.funcName, func(t *testing.T) {
			results, err := AnalyzeFile(testdataPath("opaque.go"), tc.funcName)
			if err != nil {
				t.Fatalf("AnalyzeFile: %v", err)
			}
			if len(results) == 0 || len(results[0].Params) < 1 {
				t.Fatalf("no params returned for %s", tc.funcName)
			}
			p := results[0].Params[0]
			if p.Type.Kind != "unknown" {
				t.Errorf("Type.Kind = %q, want %q (synthesizable types must not be opaque)", p.Type.Kind, "unknown")
			}
			if p.TypeName == nil {
				t.Fatalf("TypeName = nil, want %q so the planner registry can resolve a synthesized expression", tc.wantTypeName)
			}
			if *p.TypeName != tc.wantTypeName {
				t.Errorf("TypeName = %q, want %q", *p.TypeName, tc.wantTypeName)
			}
		})
	}
}

// TestAnalyzeHTTPRequestParamIsSymbolicString is the str-e41w regression: a
// direct *http.Request param is reported as a symbolic string (Kind "str")
// rather than the "unknown" runtime-value contract, so the explorer/solver
// generate request body payloads. The canonical *http.Request spelling is still
// carried on TypeName so the wrapper can wrap the symbolic body via
// httptest.NewRequest.
func TestAnalyzeHTTPRequestParamIsSymbolicString(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("opaque.go"), "AcceptsRequestPointer")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) == 0 || len(results[0].Params) < 1 {
		t.Fatalf("no params returned for AcceptsRequestPointer")
	}
	p := results[0].Params[0]
	if p.Type.Kind != "str" {
		t.Errorf("Type.Kind = %q, want %q (*http.Request body must be symbolic)", p.Type.Kind, "str")
	}
	if p.TypeName == nil || *p.TypeName != "*http.Request" {
		t.Fatalf("TypeName = %v, want %q so the wrapper can build the request from the symbolic body", p.TypeName, "*http.Request")
	}
}

func TestAnalyzeSynthesizableWazeroRuntime(t *testing.T) {
	results, err := AnalyzeFile(filepath.Join("testdata", "wazero_project", "wazero.go"), "AcceptsWazeroRuntime")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) == 0 || len(results[0].Params) < 1 {
		t.Fatalf("no params returned for AcceptsWazeroRuntime")
	}
	p := results[0].Params[0]
	if p.Type.Kind != "unknown" {
		t.Errorf("Type.Kind = %q, want unknown", p.Type.Kind)
	}
	if p.TypeName == nil {
		t.Fatalf("TypeName = nil, want wazero.Runtime")
	}
	if *p.TypeName != "wazero.Runtime" {
		t.Errorf("TypeName = %q, want wazero.Runtime", *p.TypeName)
	}
}

func TestAnalyzeSynthesizableWazeroCompiledModule(t *testing.T) {
	results, err := AnalyzeFile(filepath.Join("testdata", "wazero_project", "wazero.go"), "AcceptsCompiledModule")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) == 0 || len(results[0].Params) < 1 {
		t.Fatalf("no params returned for AcceptsCompiledModule")
	}
	p := results[0].Params[0]
	if p.Type.Kind != "unknown" {
		t.Errorf("Type.Kind = %q, want unknown", p.Type.Kind)
	}
	if p.TypeName == nil {
		t.Fatalf("TypeName = nil, want wazero.CompiledModule")
	}
	if *p.TypeName != "wazero.CompiledModule" {
		t.Errorf("TypeName = %q, want wazero.CompiledModule", *p.TypeName)
	}
}

func TestAnalyzeStructFieldPreservesWazeroRuntimeType(t *testing.T) {
	results, err := AnalyzeFile(filepath.Join("testdata", "wazero_project", "wazero.go"), "AcceptsRunner")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) == 0 || len(results[0].Params) != 1 {
		t.Fatalf("unexpected params for AcceptsRunner: %+v", results)
	}
	p := results[0].Params[0]
	if p.Type.Kind != "object" {
		t.Fatalf("runner type kind = %q, want object", p.Type.Kind)
	}
	if len(p.Type.Fields) != 1 {
		t.Fatalf("runner fields = %+v, want exactly rt", p.Type.Fields)
	}
	rtField := p.Type.Fields[0]
	if rtField.Name != "rt" {
		t.Fatalf("field name = %q, want rt", rtField.Name)
	}
	if rtField.Type.Kind != "unknown" || rtField.Type.Label != "wazero.Runtime" {
		t.Fatalf("rt field type = %+v, want unknown wazero.Runtime", rtField.Type)
	}
}

func TestAnalyzeStructFieldPreservesWazeroCompiledModuleType(t *testing.T) {
	results, err := AnalyzeFile(filepath.Join("testdata", "wazero_project", "wazero.go"), "AcceptsGenerator")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) == 0 || len(results[0].Params) != 1 {
		t.Fatalf("unexpected params for AcceptsGenerator: %+v", results)
	}
	p := results[0].Params[0]
	if p.Type.Kind != "object" {
		t.Fatalf("generator type kind = %q, want object", p.Type.Kind)
	}
	if len(p.Type.Fields) != 1 {
		t.Fatalf("generator fields = %+v, want exactly compiled", p.Type.Fields)
	}
	compiledField := p.Type.Fields[0]
	if compiledField.Name != "compiled" {
		t.Fatalf("field name = %q, want compiled", compiledField.Name)
	}
	if compiledField.Type.Kind != "unknown" || compiledField.Type.Label != "wazero.CompiledModule" {
		t.Fatalf("compiled field type = %+v, want unknown wazero.CompiledModule", compiledField.Type)
	}
}

func TestAnalyzeTemplateHolderDoesNotExposeParseNode(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("opaque.go"), "AcceptsTemplateHolder")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) == 0 || len(results[0].Params) != 1 {
		t.Fatalf("unexpected params for AcceptsTemplateHolder: %+v", results)
	}
	p := results[0].Params[0]
	if containsTypeLabel(p.Type, "parse.Node") {
		t.Fatalf("template holder exposed parse.Node internals: %+v", p.Type)
	}
}

func containsTypeLabel(t TypeInfo, needle string) bool {
	if strings.Contains(t.Label, needle) {
		return true
	}
	if t.Element != nil && containsTypeLabel(*t.Element, needle) {
		return true
	}
	if t.Inner != nil && containsTypeLabel(*t.Inner, needle) {
		return true
	}
	for _, field := range t.Fields {
		if containsTypeLabel(field.Type, needle) {
			return true
		}
	}
	for _, variant := range t.Variants {
		if containsTypeLabel(variant, needle) {
			return true
		}
	}
	return false
}

func TestAnalyzePlainInterfaceReturnsUnknown(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("opaque.go"), "AcceptsPlainInterface")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if fn.Params[0].Type.Kind != "unknown" {
		t.Errorf("plain interface type = %q, want unknown", fn.Params[0].Type.Kind)
	}
	if fn.Params[0].Type.Label != "interface" {
		t.Errorf("plain interface label = %q, want interface", fn.Params[0].Type.Label)
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

func TestExtractLiterals_PrioritizesSwitchCaseStrings(t *testing.T) {
	src := `package p

func choose(args []string) error {
	if len(args) == 0 {
		return errors.New("choose requires list, create, or delete")
	}
	switch args[0] {
	case "list":
		return nil
	case "create":
		return nil
	case "delete":
		return nil
	default:
		return fmt.Errorf("unknown command %q", args[0])
	}
}`
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "literal_priority.go", src, 0)
	if err != nil {
		t.Fatalf("ParseFile: %v", err)
	}
	lits := extractLiterals(firstFuncDecl(t, file), file)
	strs := filterLiterals(lits, "str")
	if len(strs) < 3 {
		t.Fatalf("got %d string literals, want at least three: %v", len(strs), strs)
	}
	for i, want := range []string{"list", "create", "delete"} {
		if strs[i].Value != want {
			t.Fatalf("strs[%d].Value = %q, want %q; literals=%v", i, strs[i].Value, want, strs)
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

func TestAnalyzeConfiguredRuntimeValueBypassesStaticOpacity(t *testing.T) {
	moduleRoot := t.TempDir()
	if err := os.WriteFile(filepath.Join(moduleRoot, "go.mod"), []byte("module example.com/configured\n\ngo 1.26\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}
	if err := os.MkdirAll(filepath.Join(moduleRoot, ".shatter"), 0o755); err != nil {
		t.Fatalf("mkdir .shatter: %v", err)
	}
	configBody := `
go_runtime_values:
  "configured.Secret":
    expression: configured.NewSecretForShatter()
    imports:
      - example.com/configured
`
	if err := os.WriteFile(filepath.Join(moduleRoot, ".shatter", "config.yaml"), []byte(configBody), 0o644); err != nil {
		t.Fatalf("write config: %v", err)
	}
	sourcePath := filepath.Join(moduleRoot, "secret.go")
	source := `package configured

type Secret struct {
	hidden int
}

func UseSecret(s Secret) int {
	return s.hidden
}
`
	if err := os.WriteFile(sourcePath, []byte(source), 0o644); err != nil {
		t.Fatalf("write source: %v", err)
	}

	fns, err := AnalyzeFile(sourcePath, "UseSecret")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(fns) != 1 || len(fns[0].Params) != 1 {
		t.Fatalf("analysis shape = %+v, want one function with one param", fns)
	}
	p := fns[0].Params[0]
	if p.Type.Kind != "unknown" {
		t.Fatalf("Type.Kind = %q, want unknown for configured runtime value", p.Type.Kind)
	}
	if p.Type.Label != "configured.Secret" {
		t.Errorf("Type.Label = %q, want configured.Secret", p.Type.Label)
	}
	if p.Type.StaticOpacity != "" {
		t.Errorf("StaticOpacity = %q, want empty", p.Type.StaticOpacity)
	}
	if p.TypeName == nil || *p.TypeName != "configured.Secret" {
		t.Fatalf("TypeName = %v, want configured.Secret", p.TypeName)
	}
}

// --- Medium-confidence opacity heuristics ---

func TestMediumOpacityHeuristics(t *testing.T) {
	// NOTE: Heuristic 1 (InfrastructurePackage) requires external packages from
	// known import path prefixes. Since single-file analysis only resolves types
	// available via importer.Default() in the current environment, this heuristic
	// is tested at the unit level via isMediumOpaqueGoType, not via file analysis.
	// Heuristics 2 and 3 operate on types declared in the analyzed file itself.

	fns, err := AnalyzeFile(testdataPath("medium_opaque.go"), "UseMediumOpaqueTypes")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(fns) == 0 {
		t.Fatal("no functions returned")
	}
	fn := fns[0]
	if len(fn.Params) != 3 {
		t.Fatalf("expected 3 params, got %d", len(fn.Params))
	}

	// Param a: MediumOpaque1 has Close() error → closeable_interface
	pa := fn.Params[0]
	if pa.Type.Kind != "opaque" {
		t.Errorf("param a: kind = %q, want opaque", pa.Type.Kind)
	}
	if pa.Type.MediumOpacity != "closeable_interface" {
		t.Errorf("param a: medium_opacity = %q, want closeable_interface", pa.Type.MediumOpacity)
	}
	if pa.Type.StaticOpacity != "" {
		t.Errorf("param a: static_opacity should be empty, got %q", pa.Type.StaticOpacity)
	}

	// Param b: MediumOpaque2 has fd field → native_handle_field
	pb := fn.Params[1]
	if pb.Type.Kind != "opaque" {
		t.Errorf("param b: kind = %q, want opaque", pb.Type.Kind)
	}
	if pb.Type.MediumOpacity != "native_handle_field" {
		t.Errorf("param b: medium_opacity = %q, want native_handle_field", pb.Type.MediumOpacity)
	}

	// Param c: SafeType has exported fields, no close method, no handle fields → not opaque
	pc := fn.Params[2]
	if pc.Type.Kind == "opaque" {
		t.Errorf("param c (SafeType): expected non-opaque kind, got opaque with medium_opacity=%q", pc.Type.MediumOpacity)
	}
}

// --- Induction variable / loop analysis ---

// TestLoopCanonicalIncrement verifies that a canonical for i := 0; i < n; i++
// loop is detected with the correct induction variable metadata.
func TestLoopCanonicalIncrement(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("loops.go"), "SumUpTo")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	fn := results[0]
	if len(fn.Loops) != 1 {
		t.Fatalf("loops len = %d, want 1", len(fn.Loops))
	}
	loop := fn.Loops[0]
	if loop.LoopID != 0 {
		t.Errorf("loop_id = %d, want 0", loop.LoopID)
	}
	if loop.InductionVar == nil {
		t.Fatal("induction_var is nil")
	}
	iv := loop.InductionVar
	if iv.Name != "i" {
		t.Errorf("induction_var.name = %q, want i", iv.Name)
	}
	if iv.BoundOp != "lt" {
		t.Errorf("induction_var.bound_op = %q, want lt", iv.BoundOp)
	}
	// Init should be const int 0.
	if iv.InitExpr == nil || iv.InitExpr.Kind != "const" || iv.InitExpr.Type != "int" {
		t.Errorf("init_expr = %+v, want const int", iv.InitExpr)
	}
	// Step should be const int 1.
	if iv.StepExpr == nil || iv.StepExpr.Kind != "const" || iv.StepExpr.Type != "int" {
		t.Errorf("step_expr = %+v, want const int", iv.StepExpr)
	}
	// Bound should reference param n.
	if iv.BoundExpr == nil || iv.BoundExpr.Kind != "param" || iv.BoundExpr.Name != "n" {
		t.Errorf("bound_expr = %+v, want param n", iv.BoundExpr)
	}
}

// TestLoopCanonicalStep2 verifies that for i := 0; i < n; i += 2 is detected
// with step expressed as a const int 2.
func TestLoopCanonicalStep2(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("loops.go"), "SumStep2")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Loops) != 1 {
		t.Fatalf("loops len = %d, want 1", len(fn.Loops))
	}
	iv := fn.Loops[0].InductionVar
	if iv == nil {
		t.Fatal("induction_var is nil")
	}
	if iv.BoundOp != "lt" {
		t.Errorf("bound_op = %q, want lt", iv.BoundOp)
	}
	// Step must be const int 2 (the RHS of i += 2).
	if iv.StepExpr == nil || iv.StepExpr.Kind != "const" {
		t.Fatalf("step_expr kind = %+v, want const", iv.StepExpr)
	}
}

// TestLoopBodyModifiesIV verifies that a loop whose body assigns to the
// induction variable is NOT reported as a canonical counted loop.
func TestLoopBodyModifiesIV(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("loops.go"), "ModifyIV")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Loops) != 0 {
		t.Errorf("loops len = %d, want 0 (body modifies i)", len(fn.Loops))
	}
}

// TestLoopNoCond verifies that an infinite loop (no condition) produces no LoopInfo.
func TestLoopNoCond(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("loops.go"), "NoCond")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Loops) != 0 {
		t.Errorf("loops len = %d, want 0 (no condition)", len(fn.Loops))
	}
}

// TestLoopRangeProducesNoLoopInfo verifies that range loops are not included
// in the Loops slice (range loops have no induction variable to analyze).
func TestLoopRangeProducesNoLoopInfo(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("loops.go"), "RangeOnly")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Loops) != 0 {
		t.Errorf("loops len = %d, want 0 (range loop)", len(fn.Loops))
	}
}

// TestAnalyzeZeroArgCallIncludesArgsField verifies that a branch condition
// containing a zero-argument function call serializes with "args":[] present
// in JSON, not omitted. The Rust core deserializes SymExpr::Call with a
// required args field; omitting it causes "missing field 'args'" errors.
func TestAnalyzeZeroArgCallIncludesArgsField(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("zero_arg_call.go"), "CheckReady")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	fn := results[0]
	if len(fn.Branches) == 0 {
		t.Fatal("expected at least one branch")
	}

	// Find a branch condition that contains a call SymExpr
	var found bool
	for _, br := range fn.Branches {
		if br.Condition != nil && containsCallKind(br.Condition) {
			found = true
			// Serialize the condition and verify "args" is present
			data, err := json.Marshal(br.Condition)
			if err != nil {
				t.Fatalf("marshal condition: %v", err)
			}
			jsonStr := string(data)
			if !strings.Contains(jsonStr, `"args"`) {
				t.Errorf("serialized call SymExpr missing 'args' field: %s", jsonStr)
			}
		}
	}
	if !found {
		t.Fatal("no branch condition with call kind found in CheckReady")
	}
}

func containsCallKind(expr *SymExpr) bool {
	if expr == nil {
		return false
	}
	if expr.Kind == "call" {
		return true
	}
	if containsCallKind(expr.Left) || containsCallKind(expr.Right) ||
		containsCallKind(expr.Operand) || containsCallKind(expr.Receiver) ||
		containsCallKind(expr.Condition) || containsCallKind(expr.ThenExpr) ||
		containsCallKind(expr.ElseExpr) {
		return true
	}
	for i := range expr.Args {
		if containsCallKind(&expr.Args[i]) {
			return true
		}
	}
	return false
}

// TestAnalyzeLargeFileResponse verifies that analyze responses for large files
// with many parameters are complete and valid JSON, not truncated.
//
// Regression test for: Go frontend returns truncated/malformed analyze response
// for large files, causing Rust side to fail with 'missing field args' when
// deserializing SymExpr Call variants.
func TestAnalyzeLargeFileResponse(t *testing.T) {
	filePath := testdataPath("large_file.go")

	// Analyze the large file
	results, err := AnalyzeFile(filePath, "")
	if err != nil {
		t.Fatalf("analyze failed: %v", err)
	}

	if len(results) == 0 {
		t.Fatalf("expected at least one function, got none")
	}

	// Find FunctionWithManyParams
	var analysis *FunctionAnalysis
	for i := range results {
		if results[i].Name == "FunctionWithManyParams" {
			analysis = &results[i]
			break
		}
	}

	if analysis == nil {
		t.Fatalf("FunctionWithManyParams not found in analysis results")
	}

	// Verify params are present - the bug would cause params to be empty
	// or the field to be missing when the response is truncated
	if len(analysis.Params) == 0 {
		t.Errorf("expected params, got none - response may be truncated")
	}

	// Verify the response can be marshaled to JSON without error
	responseJSON, err := json.Marshal(analysis)
	if err != nil {
		t.Fatalf("failed to marshal response to JSON: %v", err)
	}

	// Verify the JSON is valid by unmarshaling it back
	var unmarshaled FunctionAnalysis
	if err := json.Unmarshal(responseJSON, &unmarshaled); err != nil {
		t.Fatalf("failed to unmarshal response JSON: %v\nJSON was: %s", err, string(responseJSON))
	}

	// Check that params survived the round-trip
	if len(unmarshaled.Params) != len(analysis.Params) {
		t.Errorf("params lost in round-trip: before=%d, after=%d",
			len(analysis.Params), len(unmarshaled.Params))
	}

	// The key test: verify branches are valid and don't have incomplete SymExpr
	// with missing args field
	for i, branch := range analysis.Branches {
		if branch.Condition != nil {
			condJSON, err := json.Marshal(branch.Condition)
			if err != nil {
				t.Errorf("branch %d: failed to marshal condition: %v", i, err)
				continue
			}

			var cond SymExpr
			if err := json.Unmarshal(condJSON, &cond); err != nil {
				t.Errorf("branch %d: failed to unmarshal condition: %v\nJSON: %s",
					i, err, string(condJSON))
			}
		}
	}

	// Verify response JSON size is reasonable (not truncated)
	if len(responseJSON) < 500 {
		t.Logf("warning: response JSON is small (%d bytes), may be truncated", len(responseJSON))
	}
}

// TestSymExprArgsNeverNull constructs SymExprs via each builder function and
// verifies the serialized JSON never contains "args":null.
func TestSymExprArgsNeverNull(t *testing.T) {
	params := map[string]bool{"x": true, "y": true}

	// Helper: marshal a SymExpr and check args is never null
	checkArgsNotNull := func(t *testing.T, label string, expr *SymExpr) {
		t.Helper()
		if expr == nil {
			t.Errorf("%s: returned nil SymExpr", label)
			return
		}
		data, err := json.Marshal(expr)
		if err != nil {
			t.Errorf("%s: marshal error: %v", label, err)
			return
		}
		jsonStr := string(data)
		if strings.Contains(jsonStr, `"args":null`) {
			t.Errorf("%s: contains \"args\":null in JSON: %s", label, jsonStr)
		}
	}

	// identSymExpr — param
	ident := ast.Ident{Name: "x"}
	checkArgsNotNull(t, "identSymExpr(param)", identSymExpr(&ident, params))

	// identSymExpr — unknown
	identUnk := ast.Ident{Name: "z"}
	checkArgsNotNull(t, "identSymExpr(unknown)", identSymExpr(&identUnk, params))

	// selectorSymExpr — param path
	sel := ast.SelectorExpr{X: &ast.Ident{Name: "x"}, Sel: &ast.Ident{Name: "Field"}}
	checkArgsNotNull(t, "selectorSymExpr(param)", selectorSymExpr(&sel, params))

	// selectorSymExpr — unknown
	selUnk := ast.SelectorExpr{X: &ast.Ident{Name: "z"}, Sel: &ast.Ident{Name: "Field"}}
	checkArgsNotNull(t, "selectorSymExpr(unknown)", selectorSymExpr(&selUnk, params))

	// litSymExpr — int
	litInt := ast.BasicLit{Kind: token.INT, Value: "42"}
	checkArgsNotNull(t, "litSymExpr(int)", litSymExpr(&litInt))

	// litSymExpr — float
	litFloat := ast.BasicLit{Kind: token.FLOAT, Value: "3.14"}
	checkArgsNotNull(t, "litSymExpr(float)", litSymExpr(&litFloat))

	// litSymExpr — string
	litStr := ast.BasicLit{Kind: token.STRING, Value: `"hello"`}
	checkArgsNotNull(t, "litSymExpr(string)", litSymExpr(&litStr))

	// litSymExpr — unknown token
	litImag := ast.BasicLit{Kind: token.IMAG, Value: "1i"}
	checkArgsNotNull(t, "litSymExpr(unknown)", litSymExpr(&litImag))

	// buildBinOp
	binExpr := ast.BinaryExpr{X: &ast.Ident{Name: "x"}, Op: token.ADD, Y: &ast.Ident{Name: "y"}}
	checkArgsNotNull(t, "buildBinOp", buildBinOp(&binExpr, params))

	// buildUnOp
	unExpr := ast.UnaryExpr{Op: token.SUB, X: &ast.Ident{Name: "x"}}
	checkArgsNotNull(t, "buildUnOp", buildUnOp(&unExpr, params))

	// callSymExpr — zero args
	callZero := ast.CallExpr{Fun: &ast.Ident{Name: "foo"}, Args: nil}
	checkArgsNotNull(t, "callSymExpr(zero-args)", callSymExpr(&callZero, params))

	// callSymExpr — with args
	callWithArgs := ast.CallExpr{
		Fun:  &ast.Ident{Name: "bar"},
		Args: []ast.Expr{&ast.Ident{Name: "x"}},
	}
	checkArgsNotNull(t, "callSymExpr(with-args)", callSymExpr(&callWithArgs, params))

	// buildSwitchCaseSymExpr
	var tag ast.Expr = &ast.Ident{Name: "x"}
	var caseExpr ast.Expr = &ast.BasicLit{Kind: token.INT, Value: "1"}
	checkArgsNotNull(t, "buildSwitchCaseSymExpr", buildSwitchCaseSymExpr(tag, caseExpr, params, nil))
}

// --- Cyclic struct regression tests (str-ipk1) ---

func TestAnalyzeCyclicStructDoesNotCrash(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("cyclic.go"), "ProcessCyclic")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	fn := results[0]
	if fn.Name != "ProcessCyclic" {
		t.Errorf("name = %q, want ProcessCyclic", fn.Name)
	}
	if len(fn.Params) != 1 {
		t.Fatalf("params len = %d, want 1", len(fn.Params))
	}
	if fn.Params[0].Type.Kind != "object" {
		t.Errorf("param type kind = %q, want object", fn.Params[0].Type.Kind)
	}
}

func TestAnalyzeSelfRefStructDoesNotCrash(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("cyclic.go"), "ProcessSelfRef")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	fn := results[0]
	if fn.Name != "ProcessSelfRef" {
		t.Errorf("name = %q, want ProcessSelfRef", fn.Name)
	}
	if len(fn.Params) != 1 {
		t.Fatalf("params len = %d, want 1", len(fn.Params))
	}
	// Pointer to self-referential struct should be nullable wrapping object
	if fn.Params[0].Type.Kind != "nullable" {
		t.Errorf("param type kind = %q, want nullable", fn.Params[0].Type.Kind)
	}
}

// --- Fields never-null tests (str-db91) ---

// TestFieldsNeverNullInJSON verifies that TypeInfo with kind "object" always
// serializes "fields" as an array, never omits it. A missing "fields" key
// causes Rust deserialization errors.
func TestFieldsNeverNullInJSON(t *testing.T) {
	// Struct with zero exported fields — structTypeInfo should still emit fields:[]
	ti := TypeInfo{Kind: "object", Fields: []ObjectField{}}
	data, err := json.Marshal(ti)
	if err != nil {
		t.Fatalf("json.Marshal: %v", err)
	}
	if !strings.Contains(string(data), `"fields"`) {
		t.Errorf("empty-fields object TypeInfo missing 'fields' key: %s", data)
	}
	if strings.Contains(string(data), `"fields":null`) {
		t.Errorf("fields serialized as null instead of []: %s", data)
	}
}

// TestStructTypeInfoFieldsNeverNull verifies structTypeInfo always produces
// non-nil Fields, even for zero-field structs.
func TestStructTypeInfoFieldsNeverNull(t *testing.T) {
	// Analyze a function that takes EmptyStruct (zero fields)
	results, err := AnalyzeFile(testdataPath("unexported_fields.go"), "ProcessEmpty")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) == 0 {
		t.Fatal("no results")
	}
	fn := results[0]
	if len(fn.Params) == 0 {
		t.Fatal("no params")
	}
	p := fn.Params[0]
	if p.Type.Kind != "object" {
		t.Fatalf("param type kind = %q, want object", p.Type.Kind)
	}
	// Serialize and verify fields key is present
	data, err := json.Marshal(p.Type)
	if err != nil {
		t.Fatalf("json.Marshal: %v", err)
	}
	if !strings.Contains(string(data), `"fields"`) {
		t.Errorf("fields key missing from serialized TypeInfo: %s", data)
	}
}

// TestAnalyzeFile_MultiFileServiceFixture verifies that the persistent
// examples/go/multi-file-service fixture loads correctly through the
// packages-based analyzer. NewGreeter's return type is declared in iface.go;
// it resolves to kind:"opaque" (interface), confirming sibling type info was
// available.
func TestAnalyzeFile_MultiFileServiceFixture(t *testing.T) {
	_, thisFile, _, _ := runtime.Caller(0)
	// thisFile is shatter-go/protocol/analyzer_test.go; two levels up is repo root.
	repoRoot := filepath.Join(filepath.Dir(thisFile), "..", "..")
	serviceFile := filepath.Join(repoRoot, "examples", "go", "multi-file-service", "service.go")

	if _, err := os.Stat(serviceFile); err != nil {
		t.Skipf("multi-file-service fixture not present: %v", err)
	}

	results, err := AnalyzeFile(serviceFile, "")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) == 0 {
		t.Fatal("expected at least one function, got none")
	}

	// Verify NewGreeter is present and its return type resolved (not empty).
	for _, fa := range results {
		if fa.Name == "NewGreeter" {
			if fa.ReturnType.Kind == "" {
				t.Errorf("NewGreeter ReturnType.Kind empty — sibling interface type not resolved")
			}
			return
		}
	}
	t.Errorf("NewGreeter not found in results; got %v", funcNames(results))
}

// TestAnalyzeFile_InternalImportPackage verifies that a file in a package that
// imports from an internal sub-package does not produce visibility errors. Under
// the old single-file typechecker this failed with internal-import visibility
// diagnostics; the packages-based loader resolves the full module graph.
func TestAnalyzeFile_InternalImportPackage(t *testing.T) {
	_, thisFile, _, _ := runtime.Caller(0)
	// thisFile is shatter-go/protocol/analyzer_test.go; two levels up is repo root.
	repoRoot := filepath.Join(filepath.Dir(thisFile), "..", "..")
	apiFile := filepath.Join(repoRoot, "examples", "go", "internal-method", "api.go")

	if _, err := os.Stat(apiFile); err != nil {
		t.Skipf("internal-method/api.go fixture not present: %v", err)
	}

	results, err := AnalyzeFile(apiFile, "Process")
	if err != nil {
		t.Fatalf("AnalyzeFile returned error — internal-import visibility violation: %v", err)
	}
	if len(results) != 1 {
		t.Fatalf("expected 1 result, got %d", len(results))
	}
	if results[0].Name != "Process" {
		t.Errorf("function name = %q, want Process", results[0].Name)
	}
	if results[0].ReturnType.Kind != "int" {
		t.Errorf("ReturnType.Kind = %q, want int", results[0].ReturnType.Kind)
	}
}

// TestTypeInfoDepthCap verifies that goTypeToTypeInfoRec does not expand struct
// nesting beyond MaxTypeInfoDepth, preventing memory blow-up on large generated
// type graphs like openapi3.T (str-eyta). Without the cap the L1→L10 chain
// would produce a TypeInfo tree of depth 10; with the cap anything below
// MaxTypeInfoDepth is represented as kind:"unknown".
func TestTypeInfoDepthCap(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("deep_type.go"), "ProcessDeep")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) == 0 {
		t.Fatal("no results")
	}
	fn := results[0]
	if len(fn.Params) == 0 {
		t.Fatal("no params")
	}
	param := fn.Params[0]
	if param.Type.Kind != "object" {
		t.Fatalf("top-level param kind = %q, want object", param.Type.Kind)
	}

	// Walk the TypeInfo tree and compute the maximum depth reached by "object"
	// nodes (structs). Beyond the cap, nodes must be "unknown" (not "object"),
	// so the tree is bounded.
	var maxObjectDepth func(ti TypeInfo, current int) int
	maxObjectDepth = func(ti TypeInfo, current int) int {
		best := current
		for _, f := range ti.Fields {
			if f.Type.Kind == "object" {
				d := maxObjectDepth(f.Type, current+1)
				if d > best {
					best = d
				}
			}
		}
		return best
	}

	got := maxObjectDepth(param.Type, 1)
	if got > MaxTypeInfoDepth {
		t.Errorf("TypeInfo struct depth = %d, want <= MaxTypeInfoDepth (%d); depth cap not enforced", got, MaxTypeInfoDepth)
	}
}

func funcNames(results []FunctionAnalysis) []string {
	names := make([]string, len(results))
	for i, r := range results {
		names[i] = r.Name
	}
	return names
}

// TestAnalyzeFile_MultiFilePackage_ResolvesSiblingTypes verifies C2's core
// acceptance criterion: a file in a multi-file package sees sibling type
// declarations through the packages-based loader. Under the old single-file
// typechecker, the return type of NewService (defined in service.go) would
// be "opaque" because the Service interface lived in iface.go.
func TestAnalyzeFile_MultiFilePackage_ResolvesSiblingTypes(t *testing.T) {
	moduleRoot := t.TempDir()
	if err := os.WriteFile(filepath.Join(moduleRoot, "go.mod"),
		[]byte("module example.com/multifile\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}
	ifaceSource := `package multifile

type Service interface {
	Name() string
}
`
	if err := os.WriteFile(filepath.Join(moduleRoot, "iface.go"), []byte(ifaceSource), 0o644); err != nil {
		t.Fatalf("write iface.go: %v", err)
	}
	serviceSource := `package multifile

type impl struct {
	name string
}

func (i *impl) Name() string { return i.name }

// NewService constructs a Service with the given name.
func NewService(name string) Service {
	return &impl{name: name}
}
`
	serviceFile := filepath.Join(moduleRoot, "service.go")
	if err := os.WriteFile(serviceFile, []byte(serviceSource), 0o644); err != nil {
		t.Fatalf("write service.go: %v", err)
	}

	results, err := AnalyzeFile(serviceFile, "NewService")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) != 1 {
		t.Fatalf("expected 1 result, got %d", len(results))
	}
	fa := results[0]
	if fa.Name != "NewService" {
		t.Fatalf("function name = %q, want NewService", fa.Name)
	}
	if len(fa.Params) != 1 || fa.Params[0].Type.Kind != "str" {
		t.Errorf("NewService param = %+v, want single str param", fa.Params)
	}
	// The return type is the Service interface declared in iface.go. The
	// analyzer resolves interfaces to kind:"opaque", which is expected.
	// The key property is that analysis completes without error and the
	// interface method set was resolvable — which is only possible when
	// sibling files contribute type info.
	if fa.ReturnType.Kind == "" {
		t.Errorf("ReturnType.Kind empty — sibling types did not resolve")
	}
}

// TestAnalyzeFile_ExcludesSyntheticPackageInit (str-qo1.8) verifies that
// `func init()` declarations are not surfaced as executable targets, while
// regular free functions in the same file still are. Multiple init
// declarations across the package must all be filtered.
func TestAnalyzeFile_ExcludesSyntheticPackageInit(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("init_funcs.go"), "")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	for _, fa := range results {
		if fa.Name == "init" {
			t.Errorf("AnalyzeFile surfaced synthetic package init as a target")
		}
	}
	var sawPostInit bool
	for _, fa := range results {
		if fa.Name == "PostInit" {
			sawPostInit = true
		}
	}
	if !sawPostInit {
		names := make([]string, 0, len(results))
		for _, r := range results {
			names = append(names, r.Name)
		}
		t.Errorf("AnalyzeFile dropped PostInit; got names: %s", strings.Join(names, ","))
	}
}

// TestAnalyzeFile_ExcludesMainEntrypoint (str-jeen.55) verifies that a
// `package main` `func main()` declaration is not surfaced as an executable
// target. Executing main directly produces "launcher: subprocess exited
// unexpectedly" failures whenever the body invokes os.Exit / log.Fatal,
// which Zolem broad-run scans were misclassifying as launcher infrastructure
// failures. Non-`main` free functions in the same package must remain
// discoverable so helpers in CLI packages are still explorable.
func TestAnalyzeFile_ExcludesMainEntrypoint(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("main_funcs/main_funcs.go"), "")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	for _, fa := range results {
		if fa.Name == "main" {
			t.Errorf("AnalyzeFile surfaced package-main entrypoint as a target")
		}
	}
	var sawHelper bool
	for _, fa := range results {
		if fa.Name == "Helper" {
			sawHelper = true
		}
	}
	if !sawHelper {
		names := make([]string, 0, len(results))
		for _, r := range results {
			names = append(names, r.Name)
		}
		t.Errorf("AnalyzeFile dropped Helper; got names: %s", strings.Join(names, ","))
	}
}

// TestAnalyzeFile_DistinguishesSameNameMethodsAcrossReceivers is the
// str-fuhw.1.1 regression: two methods named Write on different receivers
// in the same file must surface as two distinct FunctionAnalysis entries
// with distinct Name values, so scan internals keyed on
// "<source_file>::<name>" do not collapse them. Before str-fuhw.1.1 the
// analyzer set Name = fn.Name.Name (bare), so both methods carried Name
// = "Write" and downstream call-graph and orchestrator maps overwrote
// one with the other.
//
// The fix is approach (a) from the issue: shatter-go emits the
// receiver-decorated qualified name (e.g. "(*FileWriter).Write") for
// methods, leaving free functions on the bare name. The qualified form
// matches what BuildDiscoveredTarget already publishes via
// DiscoveredTarget.QualifiedName, and matches Go's own method-value
// notation.
func TestAnalyzeFile_DistinguishesSameNameMethodsAcrossReceivers(t *testing.T) {
	results, err := AnalyzeFile(testdataPath("duplicate_method_names.go"), "")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}

	const wantFileWriterWrite = "(*FileWriter).Write"
	const wantBufferWriterWrite = "(*BufferWriter).Write"

	names := make(map[string]int, len(results))
	for _, r := range results {
		names[r.Name]++
	}

	if names[wantFileWriterWrite] != 1 {
		t.Errorf("expected exactly one analysis named %q; got names: %v",
			wantFileWriterWrite, names)
	}
	if names[wantBufferWriterWrite] != 1 {
		t.Errorf("expected exactly one analysis named %q; got names: %v",
			wantBufferWriterWrite, names)
	}
	if names["Write"] != 0 {
		t.Errorf("methods must not surface as bare %q (str-fuhw.1.1 ambiguity); got names: %v",
			"Write", names)
	}
}
