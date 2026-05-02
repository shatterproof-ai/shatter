package instrument

import (
	"fmt"
	"go/parser"
	"go/printer"
	"go/token"
	"strings"
	"testing"
)

func transformSource(t *testing.T, src string, funcName *string) (string, int) {
	t.Helper()
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, "test.go", src, 0)
	if err != nil {
		t.Fatalf("parse error: %v", err)
	}
	branchCount := transformFile(fset, file, funcName)
	var buf strings.Builder
	if err := printer.Fprint(&buf, fset, file); err != nil {
		t.Fatalf("printer error: %v", err)
	}
	return buf.String(), branchCount
}

func TestLineRecordingInsertedBeforeStatements(t *testing.T) {
	src := `package main

func F() {
	x := 1
	y := 2
}
`
	out, _ := transformSource(t, src, nil)
	if count := strings.Count(out, "__shatter_record_line"); count < 2 {
		t.Errorf("expected at least 2 line record calls, got %d\noutput:\n%s", count, out)
	}
}

func TestIfConditionWrapped(t *testing.T) {
	src := `package main

func F(x int) {
	if x > 0 {
		_ = x
	}
}
`
	out, branchCount := transformSource(t, src, nil)
	if !strings.Contains(out, "__shatter_record_branch") {
		t.Errorf("if condition not wrapped\noutput:\n%s", out)
	}
	if branchCount != 1 {
		t.Errorf("branchCount = %d, want 1", branchCount)
	}
}

func TestElseBranchRecorded(t *testing.T) {
	src := `package main

func F(x int) {
	if x > 0 {
		_ = 1
	} else {
		_ = 2
	}
}
`
	out, branchCount := transformSource(t, src, nil)
	// If with else: 1 branch for the condition, line records in both blocks
	if branchCount != 1 {
		t.Errorf("branchCount = %d, want 1", branchCount)
	}
	if !strings.Contains(out, "__shatter_record_branch") {
		t.Errorf("branch not recorded\noutput:\n%s", out)
	}
}

func TestSwitchCasesRecorded(t *testing.T) {
	src := `package main

func F(x int) {
	switch x {
	case 1:
		_ = 1
	case 2:
		_ = 2
	default:
		_ = 3
	}
}
`
	out, branchCount := transformSource(t, src, nil)
	if branchCount != 3 {
		t.Errorf("branchCount = %d, want 3 (one per case clause)", branchCount)
	}
	branchCalls := strings.Count(out, "__shatter_record_branch")
	if branchCalls != 3 {
		t.Errorf("expected 3 branch record calls, got %d\noutput:\n%s", branchCalls, out)
	}
}

// TestSwitchCaseBodyStatementsLineRecorded is the str-qo1.12 regression:
// statements inside switch case bodies (especially return statements) must
// receive __shatter_record_line calls, not just the case-entry branch
// record. Pre-fix, the switch-case-return lines never appeared in
// lines_executed and Go line coverage stayed near-zero on switch/return-
// heavy functions like Refute's detectServerKey.
func TestSwitchCaseBodyStatementsLineRecorded(t *testing.T) {
	// detectServerKey-shaped fixture: switch on a string with multiple
	// case bodies, each returning a different literal. The returns live
	// at predictable lines so we can assert the line-record arguments
	// directly.
	src := `package main

func F(ext string) string {
	switch ext {
	case ".go":
		return "go"
	case ".ts":
		return "ts"
	default:
		return ""
	}
}
`
	out, _ := transformSource(t, src, nil)

	// Each case-body return statement must have a line record. The
	// returns sit at lines 6, 8, and 10 of the fixture above (case
	// keyword on the odd line, return on the even line).
	for _, line := range []int{6, 8, 10} {
		want := fmt.Sprintf("__shatter_record_line(%d)", line)
		if !strings.Contains(out, want) {
			t.Errorf("expected line record for case-body return at line %d, want %q in:\n%s",
				line, want, out)
		}
	}
}

// TestNestedSwitchCaseInstrumented verifies that a switch nested inside a
// switch case body is recursively instrumented (control-flow recursion
// reaches case bodies, not just block bodies). (str-qo1.12)
func TestNestedSwitchCaseInstrumented(t *testing.T) {
	src := `package main

func F(a, b string) string {
	switch a {
	case "x":
		switch b {
		case "y":
			return "xy"
		}
	}
	return ""
}
`
	_, branchCount := transformSource(t, src, nil)
	// Outer switch: 1 case → 1 branch record. Inner switch: 1 case → 1
	// more branch record. If the inner switch body were skipped, only
	// the outer case would be counted (branchCount == 1).
	if branchCount != 2 {
		t.Errorf("branchCount = %d, want 2 (outer + inner case)", branchCount)
	}
}


func TestForLoopConditionWrapped(t *testing.T) {
	src := `package main

func F(n int) {
	for i := 0; i < n; i++ {
		_ = i
	}
}
`
	out, branchCount := transformSource(t, src, nil)
	if branchCount != 1 {
		t.Errorf("branchCount = %d, want 1", branchCount)
	}
	if !strings.Contains(out, "__shatter_record_branch") {
		t.Errorf("for condition not wrapped\noutput:\n%s", out)
	}
}

func TestNestedIfsAllInstrumented(t *testing.T) {
	src := `package main

func F(x int, y int) {
	if x > 0 {
		if y > 0 {
			_ = 1
		}
	}
}
`
	_, branchCount := transformSource(t, src, nil)
	if branchCount != 2 {
		t.Errorf("branchCount = %d, want 2", branchCount)
	}
}

func TestBranchIDsSequential(t *testing.T) {
	src := `package main

func F(x int, y int) {
	if x > 0 {
		_ = 1
	}
	if y > 0 {
		_ = 2
	}
}
`
	out, branchCount := transformSource(t, src, nil)
	if branchCount != 2 {
		t.Errorf("branchCount = %d, want 2", branchCount)
	}
	// Branch IDs should be 0 and 1
	if !strings.Contains(out, "__shatter_record_branch(0,") {
		t.Errorf("missing branch ID 0\noutput:\n%s", out)
	}
	if !strings.Contains(out, "__shatter_record_branch(1,") {
		t.Errorf("missing branch ID 1\noutput:\n%s", out)
	}
}

func TestFuncNameFilter(t *testing.T) {
	src := `package main

func A(x int) {
	if x > 0 { _ = 1 }
}

func B(y int) {
	if y > 0 { _ = 2 }
}
`
	name := "A"
	out, branchCount := transformSource(t, src, &name)
	if branchCount != 1 {
		t.Errorf("branchCount = %d, want 1 (only A)", branchCount)
	}
	// B should not be instrumented — check that __shatter calls only appear
	// in A's context by verifying branchCount
	_ = out
}

func TestRangeStmtRecorded(t *testing.T) {
	src := `package main

func F(items []int) {
	for _, v := range items {
		_ = v
	}
}
`
	_, branchCount := transformSource(t, src, nil)
	if branchCount != 1 {
		t.Errorf("branchCount = %d, want 1", branchCount)
	}
}

func TestForLoopScopeMarkers(t *testing.T) {
	src := `package main

func F(n int) {
	for i := 0; i < n; i++ {
		_ = i
	}
}
`
	out, _ := transformSource(t, src, nil)
	if !strings.Contains(out, `__shatter_record_scope("loop_enter"`) {
		t.Errorf("missing loop_enter scope marker\noutput:\n%s", out)
	}
	if !strings.Contains(out, `__shatter_record_scope("loop_exit"`) {
		t.Errorf("missing loop_exit scope marker\noutput:\n%s", out)
	}
}

func TestRangeLoopScopeMarkers(t *testing.T) {
	src := `package main

func F(items []int) {
	for _, v := range items {
		_ = v
	}
}
`
	out, _ := transformSource(t, src, nil)
	if !strings.Contains(out, `__shatter_record_scope("loop_enter"`) {
		t.Errorf("missing loop_enter scope marker\noutput:\n%s", out)
	}
	if !strings.Contains(out, `__shatter_record_scope("loop_exit"`) {
		t.Errorf("missing loop_exit scope marker\noutput:\n%s", out)
	}
}

func TestInfiniteLoopScopeMarkers(t *testing.T) {
	src := `package main

func F() {
	for {
		break
	}
}
`
	out, _ := transformSource(t, src, nil)
	if !strings.Contains(out, `__shatter_record_scope("loop_enter"`) {
		t.Errorf("missing loop_enter scope marker for infinite loop\noutput:\n%s", out)
	}
	if !strings.Contains(out, `__shatter_record_scope("loop_exit"`) {
		t.Errorf("missing loop_exit scope marker for infinite loop\noutput:\n%s", out)
	}
}

func TestNestedLoopsDistinctIDs(t *testing.T) {
	src := `package main

func F(n int) {
	for i := 0; i < n; i++ {
		for j := 0; j < n; j++ {
			_ = i + j
		}
	}
}
`
	out, _ := transformSource(t, src, nil)
	// Outer loop gets id 0, inner loop gets id 1
	if !strings.Contains(out, `__shatter_record_scope("loop_enter", 0)`) {
		t.Errorf("missing loop_enter with id 0\noutput:\n%s", out)
	}
	if !strings.Contains(out, `__shatter_record_scope("loop_enter", 1)`) {
		t.Errorf("missing loop_enter with id 1\noutput:\n%s", out)
	}
}

func TestFunctionCallScopeMarkers(t *testing.T) {
	src := `package main

func F(x int) {
	_ = x
}
`
	out, _ := transformSource(t, src, nil)
	if !strings.Contains(out, `__shatter_record_scope("call_enter"`) {
		t.Errorf("missing call_enter scope marker\noutput:\n%s", out)
	}
	if !strings.Contains(out, `defer __shatter_record_scope("call_exit"`) {
		t.Errorf("missing defer call_exit scope marker\noutput:\n%s", out)
	}
}

func TestClosureCapturedParamNotReassigned(t *testing.T) {
	// Param x is captured by closure but NOT reassigned afterward.
	// The closure should inherit x in its param set — branch constraint should be symbolic.
	src := `package main

func F(x int) {
	f := func() bool {
		if x > 0 {
			return true
		}
		return false
	}
	_ = f()
}
`
	out, _ := transformSource(t, src, nil)
	// The closure's if-condition should reference x as a param (JSON is escaped in Go string)
	if !strings.Contains(out, `\"kind\":\"param\",\"name\":\"x\"`) {
		t.Errorf("expected param constraint in closure, got unknown\noutput:\n%s", out)
	}
}

func TestClosureCapturedParamReassignedAfter(t *testing.T) {
	// Param x is captured by closure and reassigned AFTER the closure definition.
	// The closure should NOT inherit x — branch constraint should be unknown.
	src := `package main

func F(x int) {
	f := func() bool {
		if x > 0 {
			return true
		}
		return false
	}
	x = 42
	_ = f()
}
`
	out, _ := transformSource(t, src, nil)
	// The closure's if-condition should NOT have x as a param (should be unknown)
	if strings.Contains(out, `\"kind\":\"param\",\"name\":\"x\"`) {
		t.Errorf("expected x to be excluded from closure params due to reassignment\noutput:\n%s", out)
	}
}

func TestClosureUncapturedParamReassigned(t *testing.T) {
	// Param x is reassigned but NOT captured by the closure.
	// Param y is captured but NOT reassigned.
	// The closure should inherit y but not care about x.
	src := `package main

func F(x int, y int) {
	f := func() bool {
		if y > 0 {
			return true
		}
		return false
	}
	x = 42
	_ = f()
}
`
	out, _ := transformSource(t, src, nil)
	// y should still be a param in the closure (not captured+reassigned)
	if !strings.Contains(out, `\"kind\":\"param\",\"name\":\"y\"`) {
		t.Errorf("expected y to be a param in closure\noutput:\n%s", out)
	}
}

func TestFuncLitCallScopeMarkers(t *testing.T) {
	src := `package main

func F() {
	f := func(x int) int {
		return x * 2
	}
	_ = f(1)
}
`
	out, _ := transformSource(t, src, nil)
	// Should have call_enter for the top-level function (id 0)
	// and call_enter for the func literal (id 1)
	enterCount := strings.Count(out, `__shatter_record_scope("call_enter"`)
	if enterCount < 2 {
		t.Errorf("expected at least 2 call_enter markers (func + funclit), got %d\noutput:\n%s", enterCount, out)
	}
}
