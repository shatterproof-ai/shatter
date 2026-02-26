package instrument

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func writeExecTestSource(t *testing.T, dir, filename, content string) string {
	t.Helper()
	path := filepath.Join(dir, filename)
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatalf("writing test source: %v", err)
	}
	return path
}

func TestExecuteFunctionReturnsIntResult(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func add(a int, b int) int {
	return a + b
}
`)
	result, err := ExecuteFunction(src, "add", []json.RawMessage{
		json.RawMessage("3"),
		json.RawMessage("4"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	var retVal int
	if err := json.Unmarshal(result.ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v (raw: %s)", err, string(result.ReturnValue))
	}
	if retVal != 7 {
		t.Errorf("expected return value 7, got %d", retVal)
	}
}

func TestExecuteFunctionReturnsStringResult(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func greet(name string) string {
	return "hello " + name
}
`)
	result, err := ExecuteFunction(src, "greet", []json.RawMessage{
		json.RawMessage(`"world"`),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	var retVal string
	if err := json.Unmarshal(result.ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v", err)
	}
	if retVal != "hello world" {
		t.Errorf("expected %q, got %q", "hello world", retVal)
	}
}

func TestExecuteFunctionRecordsBranches(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func classify(x int) string {
	if x > 0 {
		return "positive"
	}
	return "nonpositive"
}
`)
	result, err := ExecuteFunction(src, "classify", []json.RawMessage{
		json.RawMessage("5"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	if len(result.BranchPath) == 0 {
		t.Fatal("expected branch decisions to be recorded")
	}

	// With x=5, branch 0 (x > 0) should be taken=true
	found := false
	for _, b := range result.BranchPath {
		if b.BranchID == 0 && b.Taken {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("expected branch 0 taken=true, got: %+v", result.BranchPath)
	}

	// Should have recorded lines
	if len(result.LinesExecuted) == 0 {
		t.Error("expected lines to be recorded")
	}

	// Return value should be "positive"
	var retVal string
	if err := json.Unmarshal(result.ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v", err)
	}
	if retVal != "positive" {
		t.Errorf("expected %q, got %q", "positive", retVal)
	}
}

func TestExecuteFunctionRecordsNegativeBranch(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func classify(x int) string {
	if x > 0 {
		return "positive"
	}
	return "nonpositive"
}
`)
	result, err := ExecuteFunction(src, "classify", []json.RawMessage{
		json.RawMessage("-1"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}

	// Branch 0 should be taken=false
	found := false
	for _, b := range result.BranchPath {
		if b.BranchID == 0 && !b.Taken {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("expected branch 0 taken=false, got: %+v", result.BranchPath)
	}

	var retVal string
	if err := json.Unmarshal(result.ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v", err)
	}
	if retVal != "nonpositive" {
		t.Errorf("expected %q, got %q", "nonpositive", retVal)
	}
}

func TestExecuteFunctionErrorsOnWrongArgCount(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func add(a int, b int) int {
	return a + b
}
`)
	_, err := ExecuteFunction(src, "add", []json.RawMessage{
		json.RawMessage("3"),
	})
	if err == nil {
		t.Error("expected error for wrong argument count")
	}
}

func TestExecuteFunctionErrorsOnMissingFunction(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func add(a int, b int) int {
	return a + b
}
`)
	_, err := ExecuteFunction(src, "nonexistent", nil)
	if err == nil {
		t.Error("expected error for missing function")
	}
}

func TestExecuteFunctionHandlesNoReturnValue(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

import "fmt"

func sayHello(name string) {
	fmt.Println("hello", name)
}
`)
	result, err := ExecuteFunction(src, "sayHello", []json.RawMessage{
		json.RawMessage(`"world"`),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	// No return value expected
	if result.ReturnValue != nil {
		t.Errorf("expected nil return value, got: %s", string(result.ReturnValue))
	}
}

func TestExecuteFunctionMeasuresPerformance(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func identity(x int) int {
	return x
}
`)
	result, err := ExecuteFunction(src, "identity", []json.RawMessage{
		json.RawMessage("42"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}

	// WallTimeMs should be positive
	if result.Performance.WallTimeMs <= 0 {
		t.Errorf("expected positive wall time, got %f", result.Performance.WallTimeMs)
	}
}
