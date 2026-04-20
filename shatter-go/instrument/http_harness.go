package instrument

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"
)

// HTTPRequestInput is the JSON structure read from shatter_inputs.json for
// adapter-owned HTTP handler execution.
type HTTPRequestInput struct {
	Method  string            `json:"method"`
	Path    string            `json:"path"`
	Headers map[string]string `json:"headers"`
	Body    string            `json:"body"`
}

// HTTPResponseOutput is the JSON structure written by the HTTP harness after
// calling the handler.
type HTTPResponseOutput struct {
	Status  int                 `json:"status"`
	Headers map[string][]string `json:"headers"`
	Body    string              `json:"body"`
}

// ExecuteHTTPHandler compiles and runs a specialized harness that invokes the
// named function with a synthetic httptest request/response pair. The inputs
// slice must contain exactly 4 JSON values: [method, path, headers, body].
// Returns an ExecuteResult with the HTTP response as return_value and empty
// instrumentation fields (adapter-owned calls are not instrumented).
func ExecuteHTTPHandler(sourcePath, funcName string, inputs []json.RawMessage, capture bool) (*ExecuteResult, error) {
	// Parse the 4 synthetic inputs into an HTTPRequestInput.
	if len(inputs) != 4 {
		return nil, fmt.Errorf("http handler adapter expects 4 inputs (method, path, headers, body), got %d", len(inputs))
	}

	var reqInput HTTPRequestInput
	if err := json.Unmarshal(inputs[0], &reqInput.Method); err != nil {
		return nil, fmt.Errorf("unmarshal method: %w", err)
	}
	if err := json.Unmarshal(inputs[1], &reqInput.Path); err != nil {
		return nil, fmt.Errorf("unmarshal path: %w", err)
	}
	if err := json.Unmarshal(inputs[2], &reqInput.Headers); err != nil {
		return nil, fmt.Errorf("unmarshal headers: %w", err)
	}
	if err := json.Unmarshal(inputs[3], &reqInput.Body); err != nil {
		return nil, fmt.Errorf("unmarshal body: %w", err)
	}

	// Prepare scratch directory.
	hash := httpHarnessHash(sourcePath, funcName)
	var outputDir string
	var err error
	if isStandaloneGoFile(sourcePath) {
		outputDir, err = makeStandaloneScratchDir()
	} else {
		outputDir, err = makeModuleScratchDir(hash)
	}
	if err != nil {
		return nil, fmt.Errorf("creating output dir: %w", err)
	}
	defer os.RemoveAll(outputDir)

	// Copy source file and siblings into the scratch directory.
	srcBase := filepath.Base(sourcePath)
	srcData, err := os.ReadFile(sourcePath)
	if err != nil {
		return nil, fmt.Errorf("reading source: %w", err)
	}
	if err := os.WriteFile(filepath.Join(outputDir, srcBase), srcData, 0644); err != nil {
		return nil, fmt.Errorf("copying source: %w", err)
	}

	if !isStandaloneGoFile(sourcePath) {
		if sibErr := copySiblingGoFiles(sourcePath, outputDir); sibErr != nil {
			fmt.Fprintf(os.Stderr, "[shatter-go] warning: copying sibling Go files: %v\n", sibErr)
		}
	}

	// Rewrite package declarations to main.
	if err := rewritePackageToMain(outputDir); err != nil {
		return nil, fmt.Errorf("rewriting package: %w", err)
	}

	// Set up go.mod (copies from project root or creates minimal).
	if err := writeGoMod(outputDir, sourcePath, nil); err != nil {
		return nil, fmt.Errorf("writing go.mod: %w", err)
	}

	// Generate and write the HTTP harness main.go.
	harnessSrc := generateHTTPHarness(funcName)
	if err := os.WriteFile(filepath.Join(outputDir, "main.go"), []byte(harnessSrc), 0644); err != nil {
		return nil, fmt.Errorf("writing main.go: %w", err)
	}

	// Write inputs file.
	inputData, err := json.Marshal(reqInput)
	if err != nil {
		return nil, fmt.Errorf("marshaling input: %w", err)
	}
	if err := os.WriteFile(filepath.Join(outputDir, "shatter_inputs.json"), inputData, 0644); err != nil {
		return nil, fmt.Errorf("writing shatter_inputs.json: %w", err)
	}

	// Build the harness.
	binaryName := "shatter_http_run"
	if runtime.GOOS == "windows" {
		binaryName += ".exe"
	}
	binaryPath := filepath.Join(outputDir, binaryName)

	buildCtx, buildCancel := context.WithTimeout(context.Background(), buildTimeout())
	defer buildCancel()

	buildCmd := exec.CommandContext(buildCtx, "go", "build", "-o", binaryPath, ".")
	buildCmd.Dir = outputDir
	applyGoBuildEnv(buildCmd, sourcePath)
	buildOut, err := buildCmd.CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("build failed: %w\n%s", err, buildOut)
	}

	// Run the harness.
	execCtx, execCancel := context.WithTimeout(context.Background(), execTimeout())
	defer execCancel()

	runCmd := exec.CommandContext(execCtx, binaryPath) //nolint:gosec
	runCmd.Dir = outputDir

	wallStart := time.Now()
	output, err := runCmd.CombinedOutput()
	wallTime := time.Since(wallStart)

	if err != nil {
		return nil, fmt.Errorf("harness execution failed: %w\noutput: %s", err, output)
	}

	// Parse the harness JSON output.
	var resp HTTPResponseOutput
	if err := json.Unmarshal(output, &resp); err != nil {
		return nil, fmt.Errorf("parsing harness output: %w\nraw: %s", err, output)
	}

	retVal, err := json.Marshal(resp)
	if err != nil {
		return nil, fmt.Errorf("marshaling response: %w", err)
	}

	return &ExecuteResult{
		ReturnValue:            retVal,
		BranchPath:             []BranchDecision{},
		LinesExecuted:          []int{},
		ExternalCalls:          []ExternalCall{},
		DiscoveredDependencies: []DiscoveredDependency{},
		SideEffects:            []SideEffect{},
		ScopeEvents:            []json.RawMessage{},
		Performance: PerfMetrics{
			WallTimeMs: float64(wallTime.Milliseconds()),
		},
	}, nil
}

// generateHTTPHarness returns Go source for a main package that reads an
// HTTPRequestInput from shatter_inputs.json, calls the handler function with
// httptest infrastructure, and writes the HTTPResponseOutput as JSON to stdout.
func generateHTTPHarness(funcName string) string {
	var b strings.Builder

	b.WriteString("package main\n\n")
	b.WriteString("import (\n")
	b.WriteString("\t\"encoding/json\"\n")
	b.WriteString("\t\"fmt\"\n")
	b.WriteString("\t\"net/http\"\n")
	b.WriteString("\t\"net/http/httptest\"\n")
	b.WriteString("\t\"os\"\n")
	b.WriteString("\t\"strings\"\n")
	b.WriteString(")\n\n")

	b.WriteString("type shatterHTTPInput struct {\n")
	b.WriteString("\tMethod  string            `json:\"method\"`\n")
	b.WriteString("\tPath    string            `json:\"path\"`\n")
	b.WriteString("\tHeaders map[string]string `json:\"headers\"`\n")
	b.WriteString("\tBody    string            `json:\"body\"`\n")
	b.WriteString("}\n\n")

	b.WriteString("type shatterHTTPOutput struct {\n")
	b.WriteString("\tStatus  int                 `json:\"status\"`\n")
	b.WriteString("\tHeaders map[string][]string `json:\"headers\"`\n")
	b.WriteString("\tBody    string              `json:\"body\"`\n")
	b.WriteString("}\n\n")

	b.WriteString("func main() {\n")
	b.WriteString("\traw, err := os.ReadFile(\"shatter_inputs.json\")\n")
	b.WriteString("\tif err != nil {\n")
	b.WriteString("\t\tfmt.Fprintf(os.Stderr, \"failed to read shatter_inputs.json: %v\\n\", err)\n")
	b.WriteString("\t\tos.Exit(1)\n")
	b.WriteString("\t}\n\n")

	b.WriteString("\tvar input shatterHTTPInput\n")
	b.WriteString("\tif err := json.Unmarshal(raw, &input); err != nil {\n")
	b.WriteString("\t\tfmt.Fprintf(os.Stderr, \"failed to parse input: %v\\n\", err)\n")
	b.WriteString("\t\tos.Exit(1)\n")
	b.WriteString("\t}\n\n")

	b.WriteString("\tvar bodyReader *strings.Reader\n")
	b.WriteString("\tif input.Body != \"\" {\n")
	b.WriteString("\t\tbodyReader = strings.NewReader(input.Body)\n")
	b.WriteString("\t} else {\n")
	b.WriteString("\t\tbodyReader = strings.NewReader(\"\")\n")
	b.WriteString("\t}\n\n")

	b.WriteString("\treq := httptest.NewRequest(input.Method, input.Path, bodyReader)\n")
	b.WriteString("\tfor k, v := range input.Headers {\n")
	b.WriteString("\t\treq.Header.Set(k, v)\n")
	b.WriteString("\t}\n\n")

	b.WriteString("\trec := httptest.NewRecorder()\n")
	b.WriteString(fmt.Sprintf("\thttp.HandlerFunc(%s).ServeHTTP(rec, req)\n\n", funcName))

	b.WriteString("\tresult := rec.Result()\n")
	b.WriteString("\tdefer result.Body.Close()\n\n")

	b.WriteString("\toutput := shatterHTTPOutput{\n")
	b.WriteString("\t\tStatus:  result.StatusCode,\n")
	b.WriteString("\t\tHeaders: rec.Header(),\n")
	b.WriteString("\t\tBody:    rec.Body.String(),\n")
	b.WriteString("\t}\n\n")

	b.WriteString("\tenc := json.NewEncoder(os.Stdout)\n")
	b.WriteString("\tif err := enc.Encode(output); err != nil {\n")
	b.WriteString("\t\tfmt.Fprintf(os.Stderr, \"failed to encode output: %v\\n\", err)\n")
	b.WriteString("\t\tos.Exit(1)\n")
	b.WriteString("\t}\n")
	b.WriteString("}\n")

	return b.String()
}

// httpHarnessHash returns a deterministic hash for an HTTP handler harness.
func httpHarnessHash(sourcePath, funcName string) string {
	input := "http:" + sourcePath + "\x00" + funcName
	h := sha256.Sum256([]byte(input))
	return hex.EncodeToString(h[:8])
}
