package instrument

import (
	"context"
	"encoding/json"
	"fmt"
	"go/ast"
	"go/importer"
	"go/parser"
	"go/token"
	"go/types"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"
)

// ExecuteResult holds the output of running an instrumented function.
type ExecuteResult struct {
	ReturnValue   json.RawMessage  `json:"return_value,omitempty"`
	ThrownError   *ErrorInfo       `json:"thrown_error,omitempty"`
	BranchPath    []BranchDecision `json:"branch_path"`
	LinesExecuted []int            `json:"lines_executed"`
	Performance   PerfMetrics      `json:"performance"`
}

// ErrorInfo describes an error thrown during execution.
type ErrorInfo struct {
	ErrorType string `json:"error_type"`
	Message   string `json:"message"`
	Stack     string `json:"stack"`
}

// BranchDecision records which way a branch was taken during execution.
type BranchDecision struct {
	BranchID       int    `json:"branch_id"`
	Line           int    `json:"line"`
	Taken          bool   `json:"taken"`
	ConstraintJSON string `json:"constraint_json,omitempty"`
}

// PerfMetrics captures execution performance data.
type PerfMetrics struct {
	WallTimeMs float64 `json:"wall_time_ms"`
}

// ExecuteFunction instruments the given source file for the target function,
// generates a main harness that calls it with the given JSON inputs, compiles,
// runs, and returns the collected results.
func ExecuteFunction(sourcePath, funcName string, inputs []json.RawMessage) (*ExecuteResult, error) {
	// Analyze the function to get parameter types
	params, returnInfo, err := analyzeForExecution(sourcePath, funcName)
	if err != nil {
		return nil, fmt.Errorf("analyzing function: %w", err)
	}

	if len(inputs) != len(params) {
		return nil, fmt.Errorf("expected %d inputs for %s, got %d", len(params), funcName, len(inputs))
	}

	// Instrument the file
	outputDir, err := InstrumentFile(sourcePath, &funcName)
	if err != nil {
		return nil, fmt.Errorf("instrumenting: %w", err)
	}
	defer os.RemoveAll(outputDir)

	// Generate the main harness
	resultsPath := filepath.Join(outputDir, "shatter_results.json")
	returnPath := filepath.Join(outputDir, "shatter_return.json")
	harness, err := generateHarness(funcName, params, returnInfo, inputs, resultsPath, returnPath)
	if err != nil {
		return nil, fmt.Errorf("generating harness: %w", err)
	}

	mainPath := filepath.Join(outputDir, "main.go")
	if err := os.WriteFile(mainPath, []byte(harness), 0644); err != nil {
		return nil, fmt.Errorf("writing main.go: %w", err)
	}

	// Build the binary
	binaryName := "shatter_run"
	if runtime.GOOS == "windows" {
		binaryName += ".exe"
	}
	binaryPath := filepath.Join(outputDir, binaryName)

	buildCtx, buildCancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer buildCancel()

	buildCmd := exec.CommandContext(buildCtx, "go", "build", "-o", binaryPath, ".")
	buildCmd.Dir = outputDir
	if buildOut, err := buildCmd.CombinedOutput(); err != nil {
		return nil, fmt.Errorf("build failed: %w\n%s", err, buildOut)
	}

	// Run the binary with a timeout
	start := time.Now()
	runCtx, runCancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer runCancel()

	runCmd := exec.CommandContext(runCtx, binaryPath)
	runCmd.Dir = outputDir
	runOut, runErr := runCmd.CombinedOutput()
	wallTime := time.Since(start)

	// Parse results even if the run failed (panic may have happened after some recording)
	result := &ExecuteResult{
		BranchPath:    []BranchDecision{},
		LinesExecuted: []int{},
		Performance:   PerfMetrics{WallTimeMs: float64(wallTime.Milliseconds())},
	}

	// Try to parse the shatter recording results
	if data, err := os.ReadFile(resultsPath); err == nil {
		var recorded struct {
			LinesExecuted []int            `json:"lines_executed"`
			BranchPath    []BranchDecision `json:"branch_path"`
		}
		if err := json.Unmarshal(data, &recorded); err == nil {
			result.LinesExecuted = recorded.LinesExecuted
			result.BranchPath = recorded.BranchPath
		}
	}

	// Try to parse the return value
	if data, err := os.ReadFile(returnPath); err == nil {
		result.ReturnValue = json.RawMessage(data)
	}

	// Handle execution errors
	if runErr != nil {
		if runCtx.Err() == context.DeadlineExceeded {
			result.ThrownError = &ErrorInfo{
				ErrorType: "timeout",
				Message:   "execution timed out after 10s",
			}
		} else {
			result.ThrownError = &ErrorInfo{
				ErrorType: "runtime_error",
				Message:   runErr.Error(),
				Stack:     string(runOut),
			}
		}
	}

	return result, nil
}

// paramInfo holds a parameter's name and Go type string for harness generation.
type paramInfo struct {
	Name   string
	GoType string
}

// returnTypeInfo describes what the function returns.
type returnTypeInfo struct {
	Count  int      // number of return values
	Types  []string // Go type strings
	HasErr bool     // last return is error
}

// analyzeForExecution parses the source file and extracts parameter types
// and return type for the named function.
func analyzeForExecution(sourcePath, funcName string) ([]paramInfo, returnTypeInfo, error) {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, sourcePath, nil, parser.ParseComments)
	if err != nil {
		return nil, returnTypeInfo{}, fmt.Errorf("parsing %s: %w", sourcePath, err)
	}

	// Type-check for better type resolution
	info := &types.Info{
		Types: make(map[ast.Expr]types.TypeAndValue),
		Defs:  make(map[*ast.Ident]types.Object),
		Uses:  make(map[*ast.Ident]types.Object),
	}
	conf := types.Config{
		Importer: importer.Default(),
		Error:    func(error) {},
	}
	conf.Check(file.Name.Name, fset, []*ast.File{file}, info) //nolint:errcheck

	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok || fn.Name.Name != funcName {
			continue
		}

		params := extractParamInfo(fn, info)
		retInfo := extractReturnInfo(fn, info)
		return params, retInfo, nil
	}

	return nil, returnTypeInfo{}, fmt.Errorf("function not found: %s", funcName)
}

func extractParamInfo(fn *ast.FuncDecl, info *types.Info) []paramInfo {
	if fn.Type.Params == nil {
		return nil
	}
	var params []paramInfo
	for _, field := range fn.Type.Params.List {
		goType := resolveGoType(field.Type, info)
		for _, name := range field.Names {
			params = append(params, paramInfo{Name: name.Name, GoType: goType})
		}
	}
	return params
}

func extractReturnInfo(fn *ast.FuncDecl, info *types.Info) returnTypeInfo {
	results := fn.Type.Results
	if results == nil || len(results.List) == 0 {
		return returnTypeInfo{}
	}

	var retTypes []string
	for _, field := range results.List {
		goType := resolveGoType(field.Type, info)
		if len(field.Names) == 0 {
			retTypes = append(retTypes, goType)
		} else {
			for range field.Names {
				retTypes = append(retTypes, goType)
			}
		}
	}

	hasErr := len(retTypes) > 0 && retTypes[len(retTypes)-1] == "error"
	return returnTypeInfo{
		Count:  len(retTypes),
		Types:  retTypes,
		HasErr: hasErr,
	}
}

// resolveGoType returns the Go type string for a type expression.
func resolveGoType(expr ast.Expr, info *types.Info) string {
	if tv, ok := info.Types[expr]; ok {
		return tv.Type.String()
	}
	// Fallback to AST-based type string
	return astTypeString(expr)
}

func astTypeString(expr ast.Expr) string {
	switch e := expr.(type) {
	case *ast.Ident:
		return e.Name
	case *ast.ArrayType:
		if e.Len == nil {
			return "[]" + astTypeString(e.Elt)
		}
		return "[" + astTypeString(e.Len) + "]" + astTypeString(e.Elt)
	case *ast.StarExpr:
		return "*" + astTypeString(e.X)
	case *ast.MapType:
		return "map[" + astTypeString(e.Key) + "]" + astTypeString(e.Value)
	case *ast.SelectorExpr:
		return astTypeString(e.X) + "." + e.Sel.Name
	case *ast.InterfaceType:
		return "interface{}"
	case *ast.BasicLit:
		return e.Value
	default:
		return "interface{}"
	}
}

// generateHarness creates a main.go that deserializes inputs, calls the function,
// captures results, and writes output files.
func generateHarness(funcName string, params []paramInfo, retInfo returnTypeInfo, inputs []json.RawMessage, resultsPath, returnPath string) (string, error) {
	var b strings.Builder

	b.WriteString("package main\n\n")
	b.WriteString("import (\n")
	b.WriteString("\t\"encoding/json\"\n")
	b.WriteString("\t\"fmt\"\n")
	b.WriteString("\t\"os\"\n")
	b.WriteString(")\n\n")

	b.WriteString("func main() {\n")

	// Declare and deserialize each input parameter
	for i, p := range params {
		inputJSON, err := json.Marshal(string(inputs[i]))
		if err != nil {
			return "", fmt.Errorf("marshaling input %d: %w", i, err)
		}

		// Write the raw JSON as a string literal, then unmarshal into the typed var
		b.WriteString(fmt.Sprintf("\tvar %s %s\n", p.Name, p.GoType))
		b.WriteString(fmt.Sprintf("\tif err := json.Unmarshal([]byte(%s), &%s); err != nil {\n", inputJSON, p.Name))
		b.WriteString(fmt.Sprintf("\t\tfmt.Fprintf(os.Stderr, \"failed to unmarshal input %s: %%v\\n\", err)\n", p.Name))
		b.WriteString("\t\tos.Exit(1)\n")
		b.WriteString("\t}\n")
	}

	b.WriteString("\n")

	// Call the function
	argList := make([]string, len(params))
	for i, p := range params {
		argList[i] = p.Name
	}
	callExpr := fmt.Sprintf("%s(%s)", funcName, strings.Join(argList, ", "))

	if retInfo.Count == 0 {
		b.WriteString(fmt.Sprintf("\t%s\n", callExpr))
	} else if retInfo.Count == 1 {
		b.WriteString(fmt.Sprintf("\tresult := %s\n", callExpr))
	} else {
		// Multiple returns: capture into named vars
		retVars := make([]string, retInfo.Count)
		for i := range retInfo.Count {
			if i == retInfo.Count-1 && retInfo.HasErr {
				retVars[i] = "retErr"
			} else {
				retVars[i] = fmt.Sprintf("ret%d", i)
			}
		}
		b.WriteString(fmt.Sprintf("\t%s := %s\n", strings.Join(retVars, ", "), callExpr))

		// If last return is error, check it
		if retInfo.HasErr {
			b.WriteString("\tif retErr != nil {\n")
			b.WriteString("\t\tfmt.Fprintf(os.Stderr, \"function returned error: %v\\n\", retErr)\n")
			b.WriteString("\t}\n")
		}

		// Build a result struct for serialization
		if retInfo.Count == 1 || (retInfo.Count == 2 && retInfo.HasErr) {
			// Single meaningful return (possibly with error)
			b.WriteString(fmt.Sprintf("\tresult := ret0\n"))
		} else {
			// Multiple returns: wrap in a slice
			nonErrVars := retVars
			if retInfo.HasErr {
				nonErrVars = retVars[:len(retVars)-1]
			}
			ifaceVars := make([]string, len(nonErrVars))
			for i, v := range nonErrVars {
				ifaceVars[i] = fmt.Sprintf("any(%s)", v)
			}
			b.WriteString(fmt.Sprintf("\tresult := []any{%s}\n", strings.Join(ifaceVars, ", ")))
		}
	}

	b.WriteString("\n")

	// Dump shatter recording results
	resultsPathEscaped := strings.ReplaceAll(resultsPath, `\`, `\\`)
	b.WriteString(fmt.Sprintf("\tif err := __shatter_dump_results(%q); err != nil {\n", resultsPathEscaped))
	b.WriteString("\t\tfmt.Fprintf(os.Stderr, \"failed to dump results: %v\\n\", err)\n")
	b.WriteString("\t}\n")

	// Write return value as JSON
	if retInfo.Count > 0 {
		returnPathEscaped := strings.ReplaceAll(returnPath, `\`, `\\`)
		b.WriteString(fmt.Sprintf("\n\treturnData, err := json.Marshal(result)\n"))
		b.WriteString("\tif err != nil {\n")
		b.WriteString("\t\tfmt.Fprintf(os.Stderr, \"failed to marshal return: %v\\n\", err)\n")
		b.WriteString("\t} else {\n")
		b.WriteString(fmt.Sprintf("\t\tos.WriteFile(%q, returnData, 0644)\n", returnPathEscaped))
		b.WriteString("\t}\n")
	}

	b.WriteString("}\n")

	return b.String(), nil
}
