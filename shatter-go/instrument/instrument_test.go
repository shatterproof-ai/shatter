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

	outputDir, err := InstrumentFile(src, nil, nil)
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

// TestInstrumentAndRunClassify — instruments classify(x int), calls with x=5,
// and verifies lines_executed, branch_path (branch 0 taken), and symbolic
// constraint (bin_op gt).
func TestInstrumentAndRunClassify(t *testing.T) {
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
	outputDir, err := InstrumentFile(src, &funcName, nil)
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

	// Verify lines_executed
	if len(results.LinesExecuted) == 0 {
		t.Error("lines_executed is empty, expected recorded lines")
	}

	// Verify branch_path: branch 0 should be taken for x=5
	if len(results.BranchPath) == 0 {
		t.Fatal("branch_path is empty")
	}
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

	// Verify symbolic constraint: should be bin_op gt
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
	if constraint.Expr == nil || constraint.Expr.Kind != "bin_op" || constraint.Expr.Op != "gt" {
		t.Errorf("expected bin_op gt constraint, got: %+v", constraint.Expr)
	}
}

// TestInstrumentMainOnlyImportPreserved verifies that imports used only by
// func main() are not orphaned when main is stripped during instrumentation.
// Reproduces: walkthrough step 8 failure where "fmt" imported and not used.
func TestInstrumentMainOnlyImportPreserved(t *testing.T) {
	srcDir := t.TempDir()
	src := writeTestSource(t, srcDir, "target.go", `package main

import "fmt"

func Add(a, b int) int {
	if a > b {
		return a + b
	}
	return b - a
}

func main() {
	fmt.Println(Add(1, 2))
}
`)

	funcName := "Add"
	outputDir, err := InstrumentFile(src, &funcName, nil)
	if err != nil {
		t.Fatalf("InstrumentFile: %v", err)
	}
	defer os.RemoveAll(outputDir)

	// go vet checks for unused imports — this must pass
	cmd := exec.Command("go", "vet", "./...")
	cmd.Dir = outputDir
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("instrumented code has unused imports: %v\n%s", err, out)
	}
}

func TestInstrumentFileNotFound(t *testing.T) {
	_, err := InstrumentFile("/nonexistent/file.go", nil, nil)
	if err == nil {
		t.Error("expected error for nonexistent file")
	}
}
