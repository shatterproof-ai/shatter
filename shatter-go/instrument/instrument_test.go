package instrument

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

type testResults struct {
	LinesExecuted []int `json:"lines_executed"`
	BranchPath    []struct {
		BranchID       int    `json:"branch_id"`
		Line           int    `json:"line"`
		Taken          bool   `json:"taken"`
		ConstraintJSON string `json:"constraint_json"`
	} `json:"branch_path"`
}

func writeTestSource(t *testing.T, dir, filename, content string) string {
	t.Helper()
	path := filepath.Join(dir, filename)
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatalf("writing test source: %v", err)
	}
	return path
}

func TestInstrumentSimpleIfElseCompiles(t *testing.T) {
	srcDir := t.TempDir()
	src := writeTestSource(t, srcDir, "target.go", `package main

func classify(x int) string {
	if x > 0 {
		return "positive"
	} else {
		return "nonpositive"
	}
}
`)

	outputDir, err := InstrumentFile(src, nil)
	if err != nil {
		t.Fatalf("InstrumentFile: %v", err)
	}
	defer os.RemoveAll(outputDir)

	// Verify the output type-checks (go vet includes type checking)
	cmd := exec.Command("go", "vet", "./...")
	cmd.Dir = outputDir
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("instrumented code does not compile: %v\n%s", err, out)
	}
}

func TestInstrumentAndRunVerifiesLinesExecuted(t *testing.T) {
	srcDir := t.TempDir()
	src := writeTestSource(t, srcDir, "target.go", `package main

func classify(x int) string {
	if x > 0 {
		return "positive"
	} else {
		return "nonpositive"
	}
}
`)

	funcName := "classify"
	outputDir, err := InstrumentFile(src, &funcName)
	if err != nil {
		t.Fatalf("InstrumentFile: %v", err)
	}
	defer os.RemoveAll(outputDir)

	// Write a main that calls the function and dumps results
	resultsPath := filepath.Join(outputDir, "results.json")
	mainSrc := `package main

func main() {
	classify(5)
	__shatter_dump_results("` + resultsPath + `")
}
`
	if err := os.WriteFile(filepath.Join(outputDir, "main.go"), []byte(mainSrc), 0644); err != nil {
		t.Fatal(err)
	}

	cmd := exec.Command("go", "run", ".")
	cmd.Dir = outputDir
	out, err := cmd.CombinedOutput()
	if err != nil {
		// List files for debugging
		entries, _ := os.ReadDir(outputDir)
		for _, e := range entries {
			data, _ := os.ReadFile(filepath.Join(outputDir, e.Name()))
			t.Logf("=== %s ===\n%s", e.Name(), string(data))
		}
		t.Fatalf("go run failed: %v\n%s", err, out)
	}

	data, err := os.ReadFile(resultsPath)
	if err != nil {
		t.Fatalf("reading results: %v", err)
	}

	var results testResults
	if err := json.Unmarshal(data, &results); err != nil {
		t.Fatalf("parsing results: %v", err)
	}

	if len(results.LinesExecuted) == 0 {
		t.Error("lines_executed is empty, expected recorded lines")
	}
}

func TestInstrumentAndRunVerifiesBranchPath(t *testing.T) {
	srcDir := t.TempDir()
	src := writeTestSource(t, srcDir, "target.go", `package main

func classify(x int) string {
	if x > 0 {
		return "positive"
	} else {
		return "nonpositive"
	}
}
`)

	funcName := "classify"
	outputDir, err := InstrumentFile(src, &funcName)
	if err != nil {
		t.Fatalf("InstrumentFile: %v", err)
	}
	defer os.RemoveAll(outputDir)

	resultsPath := filepath.Join(outputDir, "results.json")
	mainSrc := `package main

func main() {
	classify(5)
	__shatter_dump_results("` + resultsPath + `")
}
`
	if err := os.WriteFile(filepath.Join(outputDir, "main.go"), []byte(mainSrc), 0644); err != nil {
		t.Fatal(err)
	}

	cmd := exec.Command("go", "run", ".")
	cmd.Dir = outputDir
	out, err := cmd.CombinedOutput()
	if err != nil {
		entries, _ := os.ReadDir(outputDir)
		for _, e := range entries {
			data, _ := os.ReadFile(filepath.Join(outputDir, e.Name()))
			t.Logf("=== %s ===\n%s", e.Name(), string(data))
		}
		t.Fatalf("go run failed: %v\n%s", err, out)
	}

	data, err := os.ReadFile(resultsPath)
	if err != nil {
		t.Fatalf("reading results: %v", err)
	}

	var results testResults
	if err := json.Unmarshal(data, &results); err != nil {
		t.Fatalf("parsing results: %v", err)
	}

	if len(results.BranchPath) == 0 {
		t.Fatal("branch_path is empty")
	}

	// With x=5, the if x > 0 branch should be taken
	found := false
	for _, b := range results.BranchPath {
		if b.BranchID == 0 && b.Taken {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("expected branch 0 to be taken=true for x=5, got: %+v", results.BranchPath)
	}
}

func TestInstrumentCapturesSymbolicConstraints(t *testing.T) {
	srcDir := t.TempDir()
	src := writeTestSource(t, srcDir, "target.go", `package main

func classify(x int) string {
	if x > 0 {
		return "positive"
	}
	return "nonpositive"
}
`)

	funcName := "classify"
	outputDir, err := InstrumentFile(src, &funcName)
	if err != nil {
		t.Fatalf("InstrumentFile: %v", err)
	}
	defer os.RemoveAll(outputDir)

	resultsPath := filepath.Join(outputDir, "results.json")
	mainSrc := `package main

func main() {
	classify(5)
	__shatter_dump_results("` + resultsPath + `")
}
`
	if err := os.WriteFile(filepath.Join(outputDir, "main.go"), []byte(mainSrc), 0644); err != nil {
		t.Fatal(err)
	}

	cmd := exec.Command("go", "run", ".")
	cmd.Dir = outputDir
	out, err := cmd.CombinedOutput()
	if err != nil {
		entries, _ := os.ReadDir(outputDir)
		for _, e := range entries {
			data, _ := os.ReadFile(filepath.Join(outputDir, e.Name()))
			t.Logf("=== %s ===\n%s", e.Name(), string(data))
		}
		t.Fatalf("go run failed: %v\n%s", err, out)
	}

	data, err := os.ReadFile(resultsPath)
	if err != nil {
		t.Fatalf("reading results: %v", err)
	}

	var results testResults
	if err := json.Unmarshal(data, &results); err != nil {
		t.Fatalf("parsing results: %v", err)
	}

	if len(results.BranchPath) == 0 {
		t.Fatal("branch_path is empty")
	}

	// The constraint_json should contain a symbolic expression
	constraintJSON := results.BranchPath[0].ConstraintJSON
	if constraintJSON == "" {
		t.Fatal("constraint_json is empty")
	}

	var constraint struct {
		Kind string `json:"kind"`
		Expr *struct {
			Kind string `json:"kind"`
			Op   string `json:"op"`
		} `json:"expr"`
	}
	if err := json.Unmarshal([]byte(constraintJSON), &constraint); err != nil {
		t.Fatalf("parsing constraint: %v", err)
	}

	if constraint.Kind != "expr" {
		t.Errorf("constraint kind = %q, want expr", constraint.Kind)
	}
	if constraint.Expr == nil || constraint.Expr.Kind != "binop" || constraint.Expr.Op != "gt" {
		t.Errorf("expected binop gt constraint, got: %+v", constraint.Expr)
	}
}

func TestInstrumentFileNotFound(t *testing.T) {
	_, err := InstrumentFile("/nonexistent/file.go", nil)
	if err == nil {
		t.Error("expected error for nonexistent file")
	}
}
