package instrument

import (
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
