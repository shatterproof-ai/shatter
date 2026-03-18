package instrument

import (
	"context"
	"encoding/json"
	"fmt"
	"go/ast"
	"go/importer"
	"go/parser"
	"go/printer"
	"go/token"
	"go/types"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"time"

	frontendtiming "github.com/shatter-dev/shatter/shatter-go/timing"
)

const defaultExecTimeout = 5 * time.Second
const defaultBuildTimeout = 30 * time.Second

// maxTimeoutSecs guards against overflow when converting float64 seconds to
// time.Duration. time.Duration is int64 nanoseconds (max ≈292 years); we cap
// at 24 hours which is well beyond any realistic execution timeout.
const maxTimeoutSecs = 86400

// Mock behavior constants matching the protocol's DefaultBehavior field.
const (
	BehaviorRepeatLast  = "repeat_last"
	BehaviorCycle       = "cycle"
	BehaviorThrowError  = "throw_error"
	BehaviorPassthrough = "passthrough"
	MockErrorPrefix     = "Mock error: "
)

// execTimeout returns the execution timeout, reading from SHATTER_EXEC_TIMEOUT
// env var (in seconds) with a fallback to defaultExecTimeout.
func execTimeout() time.Duration {
	if d, ok := parseTimeoutEnv("SHATTER_EXEC_TIMEOUT"); ok {
		return d
	}
	return defaultExecTimeout
}

// buildTimeout returns the build timeout, reading from SHATTER_BUILD_TIMEOUT
// env var (in seconds) with a fallback to defaultBuildTimeout.
func buildTimeout() time.Duration {
	if d, ok := parseTimeoutEnv("SHATTER_BUILD_TIMEOUT"); ok {
		return d
	}
	return defaultBuildTimeout
}

// parseTimeoutEnv reads an env var as seconds and returns a valid duration.
// Returns false for missing, non-numeric, non-positive, overflow, or sub-
// nanosecond values — callers fall back to their default.
func parseTimeoutEnv(key string) (time.Duration, bool) {
	s := os.Getenv(key)
	if s == "" {
		return 0, false
	}
	secs, err := strconv.ParseFloat(s, 64)
	if err != nil || secs <= 0 || math.IsInf(secs, 0) || math.IsNaN(secs) || secs >= maxTimeoutSecs {
		return 0, false
	}
	d := time.Duration(secs * float64(time.Second))
	if d <= 0 {
		return 0, false
	}
	return d, true
}

// SideEffect represents an observable side effect during execution.
type SideEffect struct {
	Kind    string `json:"kind"`
	Level   string `json:"level,omitempty"`
	Message string `json:"message,omitempty"`
}

// DiscoveredDependency represents a dependency found at execution time
// that was not covered by the provided mocks.
type DiscoveredDependency struct {
	Symbol            string `json:"symbol"`
	SourceModule      string `json:"source_module"`
	Kind              string `json:"kind"` // "unmocked_import" or "subprocess_spawn"
	IsSubprocessSpawn bool   `json:"is_subprocess_spawn"`
}

// subprocessPackages lists Go packages that spawn external processes.
var subprocessPackages = map[string]bool{
	"os/exec": true,
}

// ExecuteResult holds the output of running an instrumented function.
type ExecuteResult struct {
	ReturnValue            json.RawMessage        `json:"return_value,omitempty"`
	ThrownError            *ErrorInfo             `json:"thrown_error,omitempty"`
	BranchPath             []BranchDecision       `json:"branch_path"`
	LinesExecuted          []int                  `json:"lines_executed"`
	ExternalCalls          []ExternalCall         `json:"external_calls,omitempty"`
	DiscoveredDependencies []DiscoveredDependency `json:"discovered_dependencies,omitempty"`
	SideEffects            []SideEffect           `json:"side_effects"`
	ScopeEvents            []json.RawMessage      `json:"scope_events"`
	Performance            PerfMetrics            `json:"performance"`
}

// ExternalCall records one call to a mocked external dependency.
type ExternalCall struct {
	Symbol      string          `json:"symbol"`
	Args        json.RawMessage `json:"args"`
	ReturnValue json.RawMessage `json:"return_value"`
}

// ErrorInfo describes an error thrown during execution.
type ErrorInfo struct {
	ErrorType     string  `json:"error_type"`
	Message       string  `json:"message"`
	Stack         string  `json:"stack"`
	ErrorCategory *string `json:"error_category,omitempty"`
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
	WallTimeMs         float64 `json:"wall_time_ms"`
	CPUTimeUs          int     `json:"cpu_time_us"`
	HeapUsedBytes      int     `json:"heap_used_bytes"`
	HeapAllocatedBytes int     `json:"heap_allocated_bytes"`
}

// MockConfig specifies how to mock an external dependency during execution.
type MockConfig struct {
	Symbol           string `json:"symbol"`
	ReturnValues     []any  `json:"return_values"`
	ShouldTrackCalls bool   `json:"should_track_calls"`
	DefaultBehavior  string `json:"default_behavior"`
}

// flattenMocks extracts the first MockConfig slice from the variadic parameter.
func flattenMocks(mocks [][]MockConfig) []MockConfig {
	if len(mocks) > 0 && len(mocks[0]) > 0 {
		return mocks[0]
	}
	return nil
}

// ExecuteFunction instruments the given source file for the target function,
// generates a main harness that calls it with the given JSON inputs, compiles,
// runs, and returns the collected results.
// The mocks parameter provides mock configurations for external dependencies.
func ExecuteFunction(sourcePath, funcName string, inputs []json.RawMessage, mocks ...[]MockConfig) (*ExecuteResult, error) {
	return ExecuteFunctionWithTiming(sourcePath, funcName, inputs, nil, mocks...)
}

// ExecuteFunctionWithTiming instruments and executes a Go function while recording timing phases when requested.
func ExecuteFunctionWithTiming(sourcePath, funcName string, inputs []json.RawMessage, timing *frontendtiming.Collector, mocks ...[]MockConfig) (*ExecuteResult, error) {
	// Analyze the function to get parameter types
	finishAnalyze := timing.Start("execute.analyze")
	params, returnInfo, err := analyzeForExecution(sourcePath, funcName)
	finishAnalyze()
	if err != nil {
		return nil, fmt.Errorf("analyzing function: %w", err)
	}

	if len(inputs) != len(params) {
		return nil, fmt.Errorf("expected %d inputs for %s, got %d", len(params), funcName, len(inputs))
	}

	// Instrument the file
	finishInstrument := timing.Start("execute.instrument")
	outputDir, err := InstrumentFileWithTiming(sourcePath, &funcName, nil, timing)
	finishInstrument()
	if err != nil {
		return nil, fmt.Errorf("instrumenting: %w", err)
	}
	defer os.RemoveAll(outputDir)

	// Rewrite all Go files in the output dir to package main so the harness can call them.
	finishRewrite := timing.Start("execute.rewrite_package")
	if err := rewritePackageToMain(outputDir); err != nil {
		finishRewrite()
		return nil, fmt.Errorf("rewriting package: %w", err)
	}
	finishRewrite()

	// Generate mock support file if mocks are provided.
	activeMocks := flattenMocks(mocks)
	mocksPath := filepath.Join(outputDir, "shatter_external_calls.json")
	if len(activeMocks) > 0 {
		mockSource := generateMockFile(activeMocks, mocksPath)
		mockFilePath := filepath.Join(outputDir, "shatter_mocks.go")
		finishWriteMocks := timing.Start("execute.write_mocks")
		if err := os.WriteFile(mockFilePath, []byte(mockSource), 0644); err != nil {
			finishWriteMocks()
			return nil, fmt.Errorf("writing shatter_mocks.go: %w", err)
		}
		finishWriteMocks()
	}

	// Generate the main harness
	resultsPath := filepath.Join(outputDir, "shatter_results.json")
	returnPath := filepath.Join(outputDir, "shatter_return.json")
	perfPath := filepath.Join(outputDir, "shatter_perf.json")
	finishHarness := timing.Start("execute.generate_harness")
	harness, err := generateHarness(funcName, params, returnInfo, inputs, resultsPath, returnPath, perfPath, len(activeMocks) > 0)
	finishHarness()
	if err != nil {
		return nil, fmt.Errorf("generating harness: %w", err)
	}

	mainPath := filepath.Join(outputDir, "main.go")
	finishWriteHarness := timing.Start("execute.write_harness")
	if err := os.WriteFile(mainPath, []byte(harness), 0644); err != nil {
		finishWriteHarness()
		return nil, fmt.Errorf("writing main.go: %w", err)
	}
	finishWriteHarness()

	// Build the binary
	binaryName := "shatter_run"
	if runtime.GOOS == "windows" {
		binaryName += ".exe"
	}
	binaryPath := filepath.Join(outputDir, binaryName)

	buildCtx, buildCancel := context.WithTimeout(context.Background(), buildTimeout())
	defer buildCancel()

	finishBuild := timing.Start("execute.build")
	buildCmd := exec.CommandContext(buildCtx, "go", "build", "-o", binaryPath, ".")
	buildCmd.Dir = outputDir
	if buildOut, err := buildCmd.CombinedOutput(); err != nil {
		finishBuild()
		return nil, fmt.Errorf("build failed: %w\n%s", err, buildOut)
	}
	finishBuild()

	// Run the binary with a timeout
	start := time.Now()
	execDur := execTimeout()
	runCtx, runCancel := context.WithTimeout(context.Background(), execDur)
	defer runCancel()

	finishRun := timing.Start("execute.run")
	runCmd := exec.CommandContext(runCtx, binaryPath)
	runCmd.Dir = outputDir
	var stdoutBuf, stderrBuf strings.Builder
	runCmd.Stdout = &stdoutBuf
	runCmd.Stderr = &stderrBuf
	runErr := runCmd.Run()
	finishRun()
	wallTime := time.Since(start)

	// Parse results even if the run failed (panic may have happened after some recording)
	result := &ExecuteResult{
		BranchPath:    []BranchDecision{},
		LinesExecuted: []int{},
		SideEffects:   []SideEffect{},
		ScopeEvents:   []json.RawMessage{},
		Performance:   PerfMetrics{WallTimeMs: float64(wallTime.Milliseconds())},
	}

	// Capture stdout/stderr as structured side effects.
	if s := strings.TrimSpace(stdoutBuf.String()); s != "" {
		result.SideEffects = append(result.SideEffects, SideEffect{
			Kind: "console_output", Level: "log", Message: s,
		})
	}
	if s := strings.TrimSpace(stderrBuf.String()); s != "" {
		result.SideEffects = append(result.SideEffects, SideEffect{
			Kind: "console_output", Level: "error", Message: s,
		})
	}

	// Try to parse the shatter recording results
	finishParseResults := timing.Start("execute.parse_results")
	if data, err := os.ReadFile(resultsPath); err == nil {
		var recorded struct {
			LinesExecuted []int             `json:"lines_executed"`
			BranchPath    []BranchDecision  `json:"branch_path"`
			ScopeEvents   []json.RawMessage `json:"scope_events"`
		}
		if err := json.Unmarshal(data, &recorded); err == nil {
			result.LinesExecuted = recorded.LinesExecuted
			result.BranchPath = recorded.BranchPath
			result.ScopeEvents = recorded.ScopeEvents
		}
	}
	finishParseResults()

	// Try to parse the return value
	finishParseReturn := timing.Start("execute.parse_return")
	if data, err := os.ReadFile(returnPath); err == nil {
		result.ReturnValue = json.RawMessage(data)
	}
	finishParseReturn()

	// Try to parse external call records from mock execution
	if len(activeMocks) > 0 {
		finishParseMockCalls := timing.Start("execute.parse_mock_calls")
		if data, err := os.ReadFile(mocksPath); err == nil {
			var calls []ExternalCall
			if err := json.Unmarshal(data, &calls); err == nil {
				result.ExternalCalls = calls
			}
		}
		finishParseMockCalls()
	}

	// Discover unmocked dependencies from source imports.
	finishDiscoverDeps := timing.Start("execute.discover_dependencies")
	if discovered := discoverDependencies(sourcePath, activeMocks); len(discovered) > 0 {
		result.DiscoveredDependencies = discovered
	}
	finishDiscoverDeps()

	// Try to parse performance metrics from the harness
	finishParsePerf := timing.Start("execute.parse_perf")
	if data, err := os.ReadFile(perfPath); err == nil {
		var perf struct {
			CPUTimeUs          int `json:"cpu_time_us"`
			HeapUsedBytes      int `json:"heap_used_bytes"`
			HeapAllocatedBytes int `json:"heap_allocated_bytes"`
		}
		if err := json.Unmarshal(data, &perf); err == nil {
			result.Performance.CPUTimeUs = perf.CPUTimeUs
			result.Performance.HeapUsedBytes = perf.HeapUsedBytes
			result.Performance.HeapAllocatedBytes = perf.HeapAllocatedBytes
		}
	}
	finishParsePerf()

	// Handle execution errors
	if runErr != nil {
		if runCtx.Err() == context.DeadlineExceeded {
			cat := "infrastructure"
			result.ThrownError = &ErrorInfo{
				ErrorType:     "timeout",
				Message:       fmt.Sprintf("execution timed out after %s", execDur),
				ErrorCategory: &cat,
			}
		} else {
			cat := "runtime"
			result.ThrownError = &ErrorInfo{
				ErrorType:     "runtime_error",
				Message:       runErr.Error(),
				Stack:         stderrBuf.String(),
				ErrorCategory: &cat,
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

		pkgName := file.Name.Name
		params := extractParamInfo(fn, info, pkgName)
		retInfo := extractReturnInfo(fn, info, pkgName)
		return params, retInfo, nil
	}

	return nil, returnTypeInfo{}, fmt.Errorf("function not found: %s", funcName)
}

func extractParamInfo(fn *ast.FuncDecl, info *types.Info, pkgName string) []paramInfo {
	if fn.Type.Params == nil {
		return nil
	}
	var params []paramInfo
	for _, field := range fn.Type.Params.List {
		goType := resolveGoType(field.Type, info, pkgName)
		for _, name := range field.Names {
			params = append(params, paramInfo{Name: name.Name, GoType: goType})
		}
	}
	return params
}

func extractReturnInfo(fn *ast.FuncDecl, info *types.Info, pkgName string) returnTypeInfo {
	results := fn.Type.Results
	if results == nil || len(results.List) == 0 {
		return returnTypeInfo{}
	}

	var retTypes []string
	for _, field := range results.List {
		goType := resolveGoType(field.Type, info, pkgName)
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
// The pkgName parameter is the source file's package name; types qualified
// with this prefix (e.g. "examples.User") are stripped to the bare name
// because the instrumented code is rewritten to package main.
func resolveGoType(expr ast.Expr, info *types.Info, pkgName string) string {
	if tv, ok := info.Types[expr]; ok {
		s := tv.Type.String()
		return stripLocalPkg(s, pkgName)
	}
	// Fallback to AST-based type string
	return astTypeString(expr)
}

// stripLocalPkg removes occurrences of "pkgName." from a type string so that
// types defined in the same package work after rewriting to package main.
func stripLocalPkg(typeStr, pkgName string) string {
	if pkgName == "" {
		return typeStr
	}
	return strings.ReplaceAll(typeStr, pkgName+".", "")
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
func generateHarness(funcName string, params []paramInfo, retInfo returnTypeInfo, inputs []json.RawMessage, resultsPath, returnPath, perfPath string, hasMocks bool) (string, error) {
	var b strings.Builder

	b.WriteString("package main\n\n")
	b.WriteString("import (\n")
	b.WriteString("\t\"encoding/json\"\n")
	b.WriteString("\t\"fmt\"\n")
	b.WriteString("\t\"os\"\n")
	b.WriteString("\t\"runtime\"\n")
	b.WriteString("\t\"time\"\n")
	b.WriteString(")\n\n")

	b.WriteString("func main() {\n")

	// If mocks are active, defer dumping external call records
	if hasMocks {
		b.WriteString("\tdefer shatterDumpMockCalls()\n\n")
	}
	b.WriteString("\tvar memBefore runtime.MemStats\n")
	b.WriteString("\truntime.ReadMemStats(&memBefore)\n")
	b.WriteString("\tcpuStart := time.Now()\n\n")

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

	// Write performance metrics
	perfPathEscaped := strings.ReplaceAll(perfPath, `\`, `\\`)
	b.WriteString("\n\tcpuElapsed := time.Since(cpuStart)\n")
	b.WriteString("\tvar memAfter runtime.MemStats\n")
	b.WriteString("\truntime.ReadMemStats(&memAfter)\n")
	b.WriteString("\tperfData, _ := json.Marshal(map[string]any{\n")
	b.WriteString("\t\t\"cpu_time_us\": cpuElapsed.Microseconds(),\n")
	b.WriteString("\t\t\"heap_used_bytes\": memAfter.HeapInuse - memBefore.HeapInuse,\n")
	b.WriteString("\t\t\"heap_allocated_bytes\": memAfter.TotalAlloc - memBefore.TotalAlloc,\n")
	b.WriteString("\t})\n")
	b.WriteString(fmt.Sprintf("\tos.WriteFile(%q, perfData, 0644)\n", perfPathEscaped))

	b.WriteString("}\n")

	return b.String(), nil
}

// generateMockFile creates a Go source file providing a mock registry and call
// tracking. Each mock symbol gets a package-level function variable that returns
// pre-configured values and records calls to a JSON file.
func generateMockFile(mocks []MockConfig, externalCallsPath string) string {
	// Check if any mock uses throw_error — the error-return variant needs "fmt".
	hasThrowError := false
	for _, m := range mocks {
		if m.DefaultBehavior == BehaviorThrowError {
			hasThrowError = true
			break
		}
	}

	var b strings.Builder

	b.WriteString("package main\n\n")
	b.WriteString("import (\n")
	b.WriteString("\t\"encoding/json\"\n")
	if hasThrowError {
		b.WriteString("\t\"fmt\"\n")
	}
	b.WriteString("\t\"os\"\n")
	b.WriteString("\t\"sync\"\n")
	b.WriteString(")\n\n")

	// Mock call record type
	b.WriteString("type shatterMockCall struct {\n")
	b.WriteString("\tSymbol      string          `json:\"symbol\"`\n")
	b.WriteString("\tArgs        json.RawMessage `json:\"args\"`\n")
	b.WriteString("\tReturnValue json.RawMessage `json:\"return_value\"`\n")
	b.WriteString("}\n\n")

	// Global call recorder
	b.WriteString("var (\n")
	b.WriteString("\tshatterMockCalls   []shatterMockCall\n")
	b.WriteString("\tshatterMockCallsMu sync.Mutex\n")
	b.WriteString(")\n\n")

	// Record helper
	b.WriteString("func shatterRecordMockCall(symbol string, args any, retVal any) {\n")
	b.WriteString("\targsJSON, _ := json.Marshal(args)\n")
	b.WriteString("\tretJSON, _ := json.Marshal(retVal)\n")
	b.WriteString("\tshatterMockCallsMu.Lock()\n")
	b.WriteString("\tshatterMockCalls = append(shatterMockCalls, shatterMockCall{\n")
	b.WriteString("\t\tSymbol:      symbol,\n")
	b.WriteString("\t\tArgs:        argsJSON,\n")
	b.WriteString("\t\tReturnValue: retJSON,\n")
	b.WriteString("\t})\n")
	b.WriteString("\tshatterMockCallsMu.Unlock()\n")
	b.WriteString("}\n\n")

	// Dump function
	pathEscaped := strings.ReplaceAll(externalCallsPath, `\`, `\\`)
	b.WriteString("func shatterDumpMockCalls() {\n")
	b.WriteString("\tshatterMockCallsMu.Lock()\n")
	b.WriteString("\tdefer shatterMockCallsMu.Unlock()\n")
	b.WriteString("\tdata, _ := json.Marshal(shatterMockCalls)\n")
	b.WriteString(fmt.Sprintf("\tos.WriteFile(%q, data, 0644)\n", pathEscaped))
	b.WriteString("}\n\n")

	// Generate a mock function variable for each symbol.
	// The mock returns the pre-configured return values in order,
	// repeating the last one when exhausted (repeat_last behavior).
	for i, mock := range mocks {
		if mock.DefaultBehavior == BehaviorPassthrough {
			continue
		}

		// Sanitize symbol to valid Go identifier
		safeName := sanitizeMockName(mock.Symbol)

		// Serialize return values as JSON array
		retValsJSON, _ := json.Marshal(mock.ReturnValues)

		b.WriteString(fmt.Sprintf("// Mock for %s\n", mock.Symbol))
		b.WriteString(fmt.Sprintf("var shatterMock%d_retvals = func() []json.RawMessage {\n", i))
		b.WriteString(fmt.Sprintf("\tvar vals []any\n"))
		b.WriteString(fmt.Sprintf("\tjson.Unmarshal([]byte(`%s`), &vals)\n", string(retValsJSON)))
		b.WriteString("\tresult := make([]json.RawMessage, len(vals))\n")
		b.WriteString("\tfor i, v := range vals {\n")
		b.WriteString("\t\tresult[i], _ = json.Marshal(v)\n")
		b.WriteString("\t}\n")
		b.WriteString("\treturn result\n")
		b.WriteString("}()\n")
		b.WriteString(fmt.Sprintf("var shatterMock%d_callIdx int\n\n", i))

		if mock.DefaultBehavior == BehaviorThrowError {
			// Generate a mock that panics with error message from return_values.
			b.WriteString(fmt.Sprintf("// ShatterMock_%s panics with error details for %s.\n", safeName, mock.Symbol))
			b.WriteString(fmt.Sprintf("func ShatterMock_%s(args ...any) any {\n", safeName))
			b.WriteString(fmt.Sprintf("\tretvals := shatterMock%d_retvals\n", i))
			b.WriteString(fmt.Sprintf("\tidx := shatterMock%d_callIdx\n", i))
			b.WriteString("\tif idx >= len(retvals) && len(retvals) > 0 {\n")
			b.WriteString("\t\tidx = len(retvals) - 1\n")
			b.WriteString("\t}\n")
			b.WriteString(fmt.Sprintf("\tshatterMock%d_callIdx++\n", i))
			b.WriteString("\n")
			b.WriteString(fmt.Sprintf("\tmsg := %q\n", MockErrorPrefix+mock.Symbol))
			b.WriteString("\tif idx < len(retvals) {\n")
			b.WriteString("\t\tvar obj map[string]any\n")
			b.WriteString("\t\tif json.Unmarshal(retvals[idx], &obj) == nil {\n")
			b.WriteString("\t\t\tif s, ok := obj[\"message\"].(string); ok && s != \"\" {\n")
			b.WriteString("\t\t\t\tmsg = s\n")
			b.WriteString("\t\t\t}\n")
			b.WriteString("\t\t}\n")
			b.WriteString("\t}\n")
			if mock.ShouldTrackCalls {
				b.WriteString(fmt.Sprintf("\tshatterRecordMockCall(%q, args, msg)\n", mock.Symbol))
			}
			b.WriteString("\tpanic(msg)\n")
			b.WriteString("}\n\n")

			// Error-return variant for Go functions that return (value, error).
			b.WriteString(fmt.Sprintf("// ShatterMockErr_%s returns an error for %s (idiomatic Go error path).\n", safeName, mock.Symbol))
			b.WriteString(fmt.Sprintf("func ShatterMockErr_%s(args ...any) (any, error) {\n", safeName))
			b.WriteString(fmt.Sprintf("\tretvals := shatterMock%d_retvals\n", i))
			b.WriteString(fmt.Sprintf("\tidx := shatterMock%d_callIdx\n", i))
			b.WriteString("\tif idx >= len(retvals) && len(retvals) > 0 {\n")
			b.WriteString("\t\tidx = len(retvals) - 1\n")
			b.WriteString("\t}\n")
			b.WriteString(fmt.Sprintf("\tshatterMock%d_callIdx++\n", i))
			b.WriteString("\n")
			b.WriteString(fmt.Sprintf("\tmsg := %q\n", MockErrorPrefix+mock.Symbol))
			b.WriteString("\tif idx < len(retvals) {\n")
			b.WriteString("\t\tvar obj map[string]any\n")
			b.WriteString("\t\tif json.Unmarshal(retvals[idx], &obj) == nil {\n")
			b.WriteString("\t\t\tif s, ok := obj[\"message\"].(string); ok && s != \"\" {\n")
			b.WriteString("\t\t\t\tmsg = s\n")
			b.WriteString("\t\t\t}\n")
			b.WriteString("\t\t}\n")
			b.WriteString("\t}\n")
			if mock.ShouldTrackCalls {
				b.WriteString(fmt.Sprintf("\tshatterRecordMockCall(%q, args, msg)\n", mock.Symbol))
			}
			b.WriteString("\treturn nil, fmt.Errorf(\"%s\", msg)\n")
			b.WriteString("}\n\n")
			continue
		}

		// Generate the mock function
		b.WriteString(fmt.Sprintf("// ShatterMock_%s returns pre-configured values for %s.\n", safeName, mock.Symbol))
		b.WriteString(fmt.Sprintf("func ShatterMock_%s(args ...any) any {\n", safeName))
		b.WriteString(fmt.Sprintf("\tretvals := shatterMock%d_retvals\n", i))
		b.WriteString(fmt.Sprintf("\tidx := shatterMock%d_callIdx\n", i))

		if mock.DefaultBehavior == BehaviorRepeatLast || mock.DefaultBehavior == "" {
			b.WriteString("\tif idx >= len(retvals) && len(retvals) > 0 {\n")
			b.WriteString("\t\tidx = len(retvals) - 1\n")
			b.WriteString("\t}\n")
		} else {
			b.WriteString("\tif len(retvals) > 0 {\n")
			b.WriteString("\t\tidx = idx % len(retvals)\n")
			b.WriteString("\t}\n")
		}

		b.WriteString(fmt.Sprintf("\tshatterMock%d_callIdx++\n", i))
		b.WriteString("\n")
		b.WriteString("\tvar retVal any\n")
		b.WriteString("\tif idx < len(retvals) {\n")
		b.WriteString("\t\tjson.Unmarshal(retvals[idx], &retVal)\n")
		b.WriteString("\t}\n")

		if mock.ShouldTrackCalls {
			b.WriteString(fmt.Sprintf("\tshatterRecordMockCall(%q, args, retVal)\n", mock.Symbol))
		}

		b.WriteString("\treturn retVal\n")
		b.WriteString("}\n\n")
	}

	// Use safeName to avoid "declared but not used" (it's used in the function name)
	b.WriteString("// Ensure shatter mock infrastructure is referenced.\n")
	b.WriteString("var _ = shatterDumpMockCalls\n")

	return b.String()
}

// sanitizeMockName converts a symbol name (e.g. "fs.readFile") to a valid Go identifier.
func sanitizeMockName(symbol string) string {
	result := make([]byte, 0, len(symbol))
	for _, c := range symbol {
		if (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_' {
			result = append(result, byte(c))
		} else {
			result = append(result, '_')
		}
	}
	return string(result)
}

// discoverDependencies inspects the source file's imports and reports any that
// are not covered by the provided mocks. Third-party packages (containing a dot
// in the import path) and known subprocess-spawning packages are reported.
func discoverDependencies(sourcePath string, mocks []MockConfig) []DiscoveredDependency {
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, sourcePath, nil, parser.ImportsOnly)
	if err != nil {
		return nil
	}

	// Build set of mocked module prefixes from mock symbols ("module:export" → "module").
	mockedModules := make(map[string]bool)
	for _, m := range mocks {
		if idx := strings.Index(m.Symbol, ":"); idx >= 0 {
			mockedModules[m.Symbol[:idx]] = true
		} else {
			mockedModules[m.Symbol] = true
		}
	}

	var deps []DiscoveredDependency
	for _, imp := range f.Imports {
		importPath := strings.Trim(imp.Path.Value, `"`)

		if mockedModules[importPath] {
			continue
		}

		if subprocessPackages[importPath] {
			deps = append(deps, DiscoveredDependency{
				Symbol:            importPath,
				SourceModule:      importPath,
				Kind:              "subprocess_spawn",
				IsSubprocessSpawn: true,
			})
			continue
		}

		// Report third-party packages (import paths containing a dot indicate
		// a domain-based module path, e.g. "github.com/...").
		if strings.Contains(importPath, ".") {
			deps = append(deps, DiscoveredDependency{
				Symbol:            importPath,
				SourceModule:      importPath,
				Kind:              "unmocked_import",
				IsSubprocessSpawn: false,
			})
		}
	}
	return deps
}

// rewritePackageToMain rewrites the package declaration in all Go files in dir
// to "package main", so the harness main.go can call functions from those files.
// Any existing func main() in source files is also stripped to avoid a redeclaration
// conflict with the harness main.go.
func rewritePackageToMain(dir string) error {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return err
	}
	for _, entry := range entries {
		if entry.IsDir() || filepath.Ext(entry.Name()) != ".go" || entry.Name() == "main.go" {
			continue
		}
		path := filepath.Join(dir, entry.Name())
		fset := token.NewFileSet()
		f, err := parser.ParseFile(fset, path, nil, parser.ParseComments)
		if err != nil {
			continue // skip unparseable files
		}

		needsRewrite := f.Name.Name != "main"
		f.Name.Name = "main"

		// Rename any func main() — the harness supplies its own. Renaming rather
		// than removing preserves imports that the original main may have used.
		for _, decl := range f.Decls {
			fd, ok := decl.(*ast.FuncDecl)
			if ok && fd.Name.Name == "main" && fd.Recv == nil {
				fd.Name.Name = "_shatter_main_"
				needsRewrite = true
			}
		}

		if !needsRewrite {
			continue
		}
		out, err := os.Create(path)
		if err != nil {
			return err
		}
		err = printer.Fprint(out, fset, f)
		out.Close()
		if err != nil {
			return err
		}
	}
	return nil
}
