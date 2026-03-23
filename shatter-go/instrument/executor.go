package instrument

import (
	"bufio"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"go/ast"
	"go/importer"
	"go/parser"
	"go/printer"
	"go/token"
	"go/types"
	"io"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
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

// isMcdcEnabled returns true when MC/DC mode is enabled (SHATTER_MCDC=1).
// Follows the same pattern as execTimeout() for SHATTER_EXEC_TIMEOUT.
func isMcdcEnabled() bool {
	return os.Getenv("SHATTER_MCDC") == "1"
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

// harnessCacheDir returns the harness cache directory from SHATTER_HARNESS_CACHE
// env var. Returns empty string if unset or empty.
func harnessCacheDir() string {
	return os.Getenv("SHATTER_HARNESS_CACHE")
}

// harnessScratchDir returns the harness scratch directory from SHATTER_HARNESS_SCRATCH
// env var. Returns empty string if unset or empty.
func harnessScratchDir() string {
	return os.Getenv("SHATTER_HARNESS_SCRATCH")
}

// isStandaloneGoFile reports whether sourcePath has no parent Go module.
// A file is standalone when no go.mod is found by walking up from its directory
// to the filesystem root — meaning the minimal fallback module would be used.
func isStandaloneGoFile(sourcePath string) bool {
	dir := filepath.Dir(sourcePath)
	for {
		if _, err := os.Stat(filepath.Join(dir, "go.mod")); err == nil {
			return false
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
		dir = parent
	}
	return true
}

// standaloneGoBuildCacheDir returns the Go build cache path for standalone
// harness builds. Uses SHATTER_HARNESS_CACHE/go/standalone/build-cache when
// the cache env var is set. Returns empty string when no cache is configured.
// The returned path is always absolute (Go requires GOCACHE to be absolute).
func standaloneGoBuildCacheDir() string {
	cache := harnessCacheDir()
	if cache == "" {
		return ""
	}
	p := filepath.Join(cache, "go", "standalone", "build-cache")
	if !filepath.IsAbs(p) {
		if abs, err := filepath.Abs(p); err == nil {
			return abs
		}
	}
	return p
}

// makeStandaloneScratchDir creates a per-request scratch directory for standalone
// Go execution. Uses SHATTER_HARNESS_SCRATCH when set; falls back to os.MkdirTemp.
// The caller is responsible for removing the directory when done.
func makeStandaloneScratchDir() (string, error) {
	if scratch := harnessScratchDir(); scratch != "" {
		dir := filepath.Join(scratch, fmt.Sprintf("go-%d-%d", os.Getpid(), time.Now().UnixMicro()))
		if err := os.MkdirAll(dir, 0755); err == nil {
			return dir, nil
		}
		// Fall through to MkdirTemp if scratch creation fails.
	}
	return os.MkdirTemp("", "shatter-instrument-*")
}

// SideEffect represents an observable side effect during execution.
// Fields correspond 1:1 with the protocol.SideEffect wire format (all 7 kinds).
// Only fields relevant to the specific Kind are populated.
type SideEffect struct {
	// Shared
	Kind string `json:"kind"`

	// console_output
	Level   string `json:"level,omitempty"`
	Message string `json:"message,omitempty"`

	// file_write
	Path    string `json:"path,omitempty"`
	Content string `json:"content,omitempty"`

	// network_request
	Method string           `json:"method,omitempty"`
	URL    string           `json:"url,omitempty"`
	Body   *json.RawMessage `json:"body,omitempty"`

	// global_mutation
	Name string `json:"name,omitempty"`

	// thrown_error
	ErrorType string  `json:"error_type,omitempty"`
	Stack     *string `json:"stack,omitempty"`

	// environment_read
	Value *string `json:"value,omitempty"`

	// global_state_change
	Variable string           `json:"variable,omitempty"`
	Before   *json.RawMessage `json:"before,omitempty"`
	After    *json.RawMessage `json:"after,omitempty"`
}

// globalVarInfo holds the name of an exported package-level variable to track.
type globalVarInfo struct {
	Name string
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

// ConditionOutcome records the result of an individual condition within a
// compound boolean decision (MC/DC analysis).
type ConditionOutcome struct {
	ConditionIndex int             `json:"condition_index"`
	Value          *bool           `json:"value"`  // nil when masked by short-circuit
	Masked         bool            `json:"masked,omitempty"`
	ConstraintJSON string          `json:"constraint_json,omitempty"`
}

// BranchDecision records which way a branch was taken during execution.
type BranchDecision struct {
	BranchID       int                `json:"branch_id"`
	Line           int                `json:"line"`
	Taken          bool               `json:"taken"`
	ConstraintJSON string             `json:"constraint_json,omitempty"`
	// Conditions holds per-condition outcomes for MC/DC analysis.
	// Present only when MC/DC mode is enabled and the decision is compound.
	Conditions     []ConditionOutcome `json:"conditions,omitempty"`
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

// harnessID uniquely identifies a compiled harness by source path, function name,
// and a hash of any mock configurations. Used as a map key for the process cache.
type harnessID struct {
	sourcePath string
	funcName   string
	mocksHash  string
}

// harnessLoopResponse is the JSON payload read from a persistent harness subprocess
// stdout (one line per execution response). Field types mirror ExecuteResult so they
// can be assigned directly without conversion.
type harnessLoopResponse struct {
	ReturnValue   json.RawMessage   `json:"return_value"`
	BranchPath    []BranchDecision  `json:"branch_path"`
	LinesExecuted []int             `json:"lines_executed"`
	ScopeEvents   []json.RawMessage `json:"scope_events"`
	SideEffects   []SideEffect      `json:"side_effects"`
	ExternalCalls []ExternalCall    `json:"external_calls,omitempty"`
	ThrownError   *ErrorInfo        `json:"thrown_error,omitempty"`
	Performance   *PerfMetrics      `json:"performance,omitempty"`
	// Error is set by the harness when it cannot process the request (not a
	// thrown function error). A non-empty value causes execute() to return an error.
	Error string `json:"error,omitempty"`
}

// persistentHarness holds a long-lived harness subprocess that can be reused
// across multiple execute calls for the same function without recompilation.
type persistentHarness struct {
	cmd      *exec.Cmd
	stdinEnc *json.Encoder
	stdin    io.WriteCloser
	stdout   *bufio.Scanner
	// dir is the instrumented output directory. It is NOT removed while the harness
	// is alive; it is cleaned up in close().
	dir        string
	discDeps   []DiscoveredDependency // cached once at spawn time
	paramCount int                    // expected number of inputs per call
	mu         sync.Mutex
}

var (
	harnessProcs   = map[harnessID]*persistentHarness{}
	harnessProcsMu sync.RWMutex
)

// computeMocksHash returns a short deterministic hash of the mock symbols so that
// different mock configurations get different harness subprocesses.
func computeMocksHash(mocks []MockConfig) string {
	if len(mocks) == 0 {
		return ""
	}
	syms := make([]string, len(mocks))
	for i, m := range mocks {
		syms[i] = m.Symbol
	}
	sort.Strings(syms)
	h := sha256.Sum256([]byte(strings.Join(syms, ",")))
	return hex.EncodeToString(h[:4])
}

func getHarness(id harnessID) *persistentHarness {
	harnessProcsMu.RLock()
	defer harnessProcsMu.RUnlock()
	return harnessProcs[id]
}

func putHarness(id harnessID, h *persistentHarness) {
	harnessProcsMu.Lock()
	defer harnessProcsMu.Unlock()
	harnessProcs[id] = h
}

func removeHarness(id harnessID) {
	harnessProcsMu.Lock()
	defer harnessProcsMu.Unlock()
	delete(harnessProcs, id)
}

// CloseAllHarnesses kills all cached harness subprocesses and removes their temp
// directories. Should be called from the shutdown handler.
func CloseAllHarnesses() {
	harnessProcsMu.Lock()
	defer harnessProcsMu.Unlock()
	for id, h := range harnessProcs {
		h.close()
		delete(harnessProcs, id)
	}
}

// close terminates the harness subprocess and removes its temp directory.
func (h *persistentHarness) close() {
	h.stdin.Close() // sends EOF to the harness loop → graceful exit
	_ = h.cmd.Wait()
	os.RemoveAll(h.dir)
}

// execute sends a single request to the persistent harness subprocess and reads
// the response. timeout governs how long we wait for the harness to respond;
// if it expires the subprocess is killed and an error is returned.
func (h *persistentHarness) execute(inputs []json.RawMessage, capture bool, timeout time.Duration) (*harnessLoopResponse, error) {
	h.mu.Lock()
	defer h.mu.Unlock()

	req := struct {
		Inputs  []json.RawMessage `json:"inputs"`
		Capture bool              `json:"capture"`
	}{Inputs: inputs, Capture: capture}
	if err := h.stdinEnc.Encode(req); err != nil {
		return nil, fmt.Errorf("writing harness request: %w", err)
	}

	type result struct {
		data []byte
		err  error
	}
	ch := make(chan result, 1)
	go func() {
		if h.stdout.Scan() {
			b := make([]byte, len(h.stdout.Bytes()))
			copy(b, h.stdout.Bytes())
			ch <- result{data: b}
		} else {
			err := h.stdout.Err()
			if err == nil {
				err = io.EOF
			}
			ch <- result{err: err}
		}
	}()

	select {
	case res := <-ch:
		if res.err != nil {
			return nil, res.err
		}
		var resp harnessLoopResponse
		if err := json.Unmarshal(res.data, &resp); err != nil {
			return nil, fmt.Errorf("parsing harness response: %w", err)
		}
		if resp.Error != "" {
			return nil, fmt.Errorf("harness error: %s", resp.Error)
		}
		return &resp, nil
	case <-time.After(timeout):
		h.cmd.Process.Kill() //nolint:errcheck
		return nil, fmt.Errorf("execution timed out after %s", timeout)
	}
}

// buildAndSpawnHarness compiles a persistent loop harness for the given function
// and starts it as a subprocess. The returned harness is ready to receive requests.
func buildAndSpawnHarness(sourcePath, funcName string, activeMocks []MockConfig, timing *frontendtiming.Collector) (*persistentHarness, error) {
	finishAnalyze := timing.Start("execute.analyze")
	params, returnInfo, err := analyzeForExecution(sourcePath, funcName)
	finishAnalyze()
	if err != nil {
		return nil, fmt.Errorf("analyzing function: %w", err)
	}
	globalVars, err := analyzeGlobalVars(sourcePath)
	if err != nil {
		globalVars = nil // non-fatal
	}
	discDeps := discoverDependencies(sourcePath, activeMocks)


	// Prepare the output directory.
	// Standalone files (no parent go.mod) use a per-request scratch dir so that
	// ephemeral build state is properly lifecycle-separated from tool-owned cache.
	// Files with a parent module fall back to MkdirTemp (semantic mode is future work).
	var outputDir string
	if isStandaloneGoFile(sourcePath) {
		outputDir, err = makeStandaloneScratchDir()
	} else {
		outputDir, err = os.MkdirTemp("", "shatter-instrument-*")
	}
	if err != nil {
		return nil, fmt.Errorf("creating output dir: %w", err)
	}
	// NOTE: do NOT defer os.RemoveAll — the harness subprocess keeps this dir alive.

	// Instrument the file into the prepared directory.
	finishInstrument := timing.Start("execute.instrument")
	err = InstrumentFileToDir(sourcePath, outputDir, &funcName, nil, timing)
	finishInstrument()
	if err != nil {
		return nil, fmt.Errorf("instrumenting: %w", err)
	}

	finishRewrite := timing.Start("execute.rewrite_package")
	if err := rewritePackageToMain(outputDir); err != nil {
		finishRewrite()
		os.RemoveAll(outputDir)
		return nil, fmt.Errorf("rewriting package: %w", err)
	}
	finishRewrite()

	if len(activeMocks) > 0 {
		mockSource := generateLoopMockFile(activeMocks)
		if err := os.WriteFile(filepath.Join(outputDir, "shatter_mocks.go"), []byte(mockSource), 0644); err != nil {
			os.RemoveAll(outputDir)
			return nil, fmt.Errorf("writing shatter_mocks.go: %w", err)
		}
	}

	finishHarness := timing.Start("execute.generate_harness")
	harness, err := generateLoopHarness(funcName, params, returnInfo, globalVars, len(activeMocks) > 0)
	finishHarness()
	if err != nil {
		os.RemoveAll(outputDir)
		return nil, fmt.Errorf("generating loop harness: %w", err)
	}
	if err := os.WriteFile(filepath.Join(outputDir, "main.go"), []byte(harness), 0644); err != nil {
		os.RemoveAll(outputDir)
		return nil, fmt.Errorf("writing main.go: %w", err)
	}

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
	// Use a project-scoped build cache for standalone files so compiled objects
	// persist across requests and survive OS temp cleanup.
	if isStandaloneGoFile(sourcePath) {
		if gocache := standaloneGoBuildCacheDir(); gocache != "" {
			buildCmd.Env = append(os.Environ(), "GOCACHE="+gocache)
		}
	}
	if buildOut, err := buildCmd.CombinedOutput(); err != nil {
		finishBuild()
		os.RemoveAll(outputDir)
		return nil, fmt.Errorf("build failed: %w\n%s", err, buildOut)
	}
	finishBuild()

	cmd := exec.Command(binaryPath) //nolint:gosec
	cmd.Dir = outputDir
	cmd.Stderr = os.Stderr // forward harness stderr (debug logs, panics) to our stderr

	stdinPipe, err := cmd.StdinPipe()
	if err != nil {
		os.RemoveAll(outputDir)
		return nil, fmt.Errorf("creating stdin pipe: %w", err)
	}
	stdoutPipe, err := cmd.StdoutPipe()
	if err != nil {
		stdinPipe.Close()
		os.RemoveAll(outputDir)
		return nil, fmt.Errorf("creating stdout pipe: %w", err)
	}
	if err := cmd.Start(); err != nil {
		stdinPipe.Close()
		os.RemoveAll(outputDir)
		return nil, fmt.Errorf("starting harness subprocess: %w", err)
	}

	scanner := bufio.NewScanner(stdoutPipe)
	scanner.Buffer(make([]byte, 4*1024*1024), 4*1024*1024) // 4 MB for large responses

	return &persistentHarness{
		cmd:        cmd,
		stdinEnc:   json.NewEncoder(stdinPipe),
		stdin:      stdinPipe,
		stdout:     scanner,
		dir:        outputDir,
		discDeps:   discDeps,
		paramCount: len(params),
	}, nil
}

// ExecuteFunction instruments the given source file for the target function,
// generates a main harness that calls it with the given JSON inputs, compiles,
// runs, and returns the collected results.
// The mocks parameter provides mock configurations for external dependencies.
// capture controls whether stdout/stderr side effects are collected.
func ExecuteFunction(sourcePath, funcName string, inputs []json.RawMessage, capture bool, mocks ...[]MockConfig) (*ExecuteResult, error) {
	return ExecuteFunctionWithTiming(sourcePath, funcName, inputs, nil, capture, mocks...)
}

// ExecuteFunctionWithTiming instruments and executes a Go function while recording
// timing phases when requested. On the first call for a (sourcePath, funcName, mocks)
// triple the harness is compiled and started as a persistent subprocess. Subsequent
// calls send a JSON request over stdin and read the JSON response from stdout,
// eliminating per-call build overhead.
// capture controls whether stdout/stderr side effects are collected.
func ExecuteFunctionWithTiming(sourcePath, funcName string, inputs []json.RawMessage, timing *frontendtiming.Collector, capture bool, mocks ...[]MockConfig) (*ExecuteResult, error) {
	activeMocks := flattenMocks(mocks)
	id := harnessID{
		sourcePath: sourcePath,
		funcName:   funcName,
		mocksHash:  computeMocksHash(activeMocks),
	}

	h := getHarness(id)
	if h == nil {
		var err error
		h, err = buildAndSpawnHarness(sourcePath, funcName, activeMocks, timing)
		if err != nil {
			return nil, err
		}
		putHarness(id, h)
	}

	if len(inputs) != h.paramCount {
		return nil, fmt.Errorf("expected %d inputs for %s, got %d", h.paramCount, funcName, len(inputs))
	}

	execDur := execTimeout()
	finishRun := timing.Start("execute.run")
	wallStart := time.Now()
	resp, err := h.execute(inputs, capture, execDur)
	wallTime := time.Since(wallStart)
	finishRun()

	if err != nil {
		// Remove the dead harness so the next call re-compiles.
		removeHarness(id)
		h.close()

		if strings.Contains(err.Error(), "timed out") {
			cat := "infrastructure"
			return &ExecuteResult{
				BranchPath:    []BranchDecision{},
				LinesExecuted: []int{},
				SideEffects:   []SideEffect{},
				ScopeEvents:   []json.RawMessage{},
				Performance:   PerfMetrics{WallTimeMs: float64(wallTime.Milliseconds())},
				ThrownError: &ErrorInfo{
					ErrorType:     "timeout",
					Message:       fmt.Sprintf("execution timed out after %s", execDur),
					ErrorCategory: &cat,
				},
			}, nil
		}
		return nil, err
	}

	result := &ExecuteResult{
		ReturnValue:            resp.ReturnValue,
		ThrownError:            resp.ThrownError,
		BranchPath:             resp.BranchPath,
		LinesExecuted:          resp.LinesExecuted,
		ExternalCalls:          resp.ExternalCalls,
		DiscoveredDependencies: h.discDeps,
		SideEffects:            resp.SideEffects,
		ScopeEvents:            resp.ScopeEvents,
		Performance:            PerfMetrics{WallTimeMs: float64(wallTime.Milliseconds())},
	}
	if resp.Performance != nil {
		result.Performance.CPUTimeUs = resp.Performance.CPUTimeUs
		result.Performance.HeapUsedBytes = resp.Performance.HeapUsedBytes
		result.Performance.HeapAllocatedBytes = resp.Performance.HeapAllocatedBytes
	}
	// Ensure non-nil slices so downstream code never has to nil-check.
	if result.BranchPath == nil {
		result.BranchPath = []BranchDecision{}
	}
	if result.LinesExecuted == nil {
		result.LinesExecuted = []int{}
	}
	if result.SideEffects == nil {
		result.SideEffects = []SideEffect{}
	}
	if result.ScopeEvents == nil {
		result.ScopeEvents = []json.RawMessage{}
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

// analyzeGlobalVars returns the exported (capitalized) package-level var declarations
// in the source file. These are candidates for global_state_change tracking.
// Constants, functions, and unexported vars are excluded.
func analyzeGlobalVars(sourcePath string) ([]globalVarInfo, error) {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, sourcePath, nil, 0)
	if err != nil {
		return nil, fmt.Errorf("parsing %s: %w", sourcePath, err)
	}

	var vars []globalVarInfo
	for _, decl := range file.Decls {
		genDecl, ok := decl.(*ast.GenDecl)
		if !ok || genDecl.Tok != token.VAR {
			continue
		}
		for _, spec := range genDecl.Specs {
			valSpec, ok := spec.(*ast.ValueSpec)
			if !ok {
				continue
			}
			for _, name := range valSpec.Names {
				// Only track exported (capitalized) names.
				if len(name.Name) > 0 && name.Name[0] >= 'A' && name.Name[0] <= 'Z' {
					vars = append(vars, globalVarInfo{Name: name.Name})
				}
			}
		}
	}
	return vars, nil
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

// generateLoopHarness creates a main.go that loops on stdin reading JSON requests
// and writes JSON responses to stdout. This replaces the single-shot generateHarness
// and allows the subprocess to be reused across many execute calls without recompilation.
//
// Request format (one JSON line per call):
//
//	{"inputs": [<json>, ...], "capture": true}
//
// Response format (one JSON line per call):
//
//	{"return_value": <json>, "branch_path": [...], "lines_executed": [...], ...}
func generateLoopHarness(funcName string, params []paramInfo, retInfo returnTypeInfo, globalVars []globalVarInfo, hasMocks bool) (string, error) {
	var b strings.Builder

	b.WriteString("package main\n\n")
	b.WriteString("import (\n")
	b.WriteString("\t\"bufio\"\n")
	b.WriteString("\t\"bytes\"\n")
	b.WriteString("\t\"encoding/json\"\n")
	b.WriteString("\t\"fmt\"\n")
	b.WriteString("\t\"io\"\n")
	b.WriteString("\t\"os\"\n")
	b.WriteString("\t\"runtime\"\n")
	b.WriteString("\t\"strings\"\n")
	b.WriteString("\t\"time\"\n")
	b.WriteString(")\n\n")

	// Inline types avoid package-level import issues.
	b.WriteString("type _hReq struct {\n")
	b.WriteString("\tInputs  []json.RawMessage `json:\"inputs\"`\n")
	b.WriteString("\tCapture bool              `json:\"capture\"`\n")
	b.WriteString("}\n\n")

	b.WriteString("type _hSideEffect struct {\n")
	b.WriteString("\tKind     string          `json:\"kind\"`\n")
	b.WriteString("\tLevel    string          `json:\"level,omitempty\"`\n")
	b.WriteString("\tMessage  string          `json:\"message,omitempty\"`\n")
	b.WriteString("\tVariable string          `json:\"variable,omitempty\"`\n")
	b.WriteString("\tBefore   json.RawMessage `json:\"before,omitempty\"`\n")
	b.WriteString("\tAfter    json.RawMessage `json:\"after,omitempty\"`\n")
	b.WriteString("}\n\n")

	b.WriteString("type _hError struct {\n")
	b.WriteString("\tErrorType     string `json:\"error_type\"`\n")
	b.WriteString("\tMessage       string `json:\"message\"`\n")
	b.WriteString("\tStack         string `json:\"stack,omitempty\"`\n")
	b.WriteString("\tErrorCategory string `json:\"error_category,omitempty\"`\n")
	b.WriteString("}\n\n")

	b.WriteString("type _hPerf struct {\n")
	b.WriteString("\tCPUTimeUs          int64 `json:\"cpu_time_us\"`\n")
	b.WriteString("\tHeapUsedBytes      int64 `json:\"heap_used_bytes\"`\n")
	b.WriteString("\tHeapAllocatedBytes int64 `json:\"heap_allocated_bytes\"`\n")
	b.WriteString("}\n\n")

	b.WriteString("type _hResp struct {\n")
	b.WriteString("\tReturnValue   json.RawMessage          `json:\"return_value,omitempty\"`\n")
	b.WriteString("\tBranchPath    []__shatterBranchDecision `json:\"branch_path\"`\n")
	b.WriteString("\tLinesExecuted []int                    `json:\"lines_executed\"`\n")
	b.WriteString("\tScopeEvents   []__shatterTraceEvent    `json:\"scope_events\"`\n")
	b.WriteString("\tSideEffects   []_hSideEffect           `json:\"side_effects\"`\n")
	if hasMocks {
		b.WriteString("\tExternalCalls []json.RawMessage        `json:\"external_calls,omitempty\"`\n")
	}
	b.WriteString("\tThrownError   *_hError                 `json:\"thrown_error,omitempty\"`\n")
	b.WriteString("\tPerformance   *_hPerf                  `json:\"performance\"`\n")
	b.WriteString("\tError         string                   `json:\"error,omitempty\"`\n")
	b.WriteString("}\n\n")

	// Suppress "declared and not used" for imports that may be unused in some configurations.
	b.WriteString("var _ = strings.TrimSpace\n")
	b.WriteString("var _ = time.Now\n\n")

	b.WriteString("func main() {\n")
	b.WriteString("\t_sc := bufio.NewScanner(os.Stdin)\n")
	b.WriteString("\t_sc.Buffer(make([]byte, 4*1024*1024), 4*1024*1024)\n")
	b.WriteString("\t_enc := json.NewEncoder(os.Stdout)\n\n")

	b.WriteString("\tfor _sc.Scan() {\n")
	b.WriteString("\t\tvar _req _hReq\n")
	b.WriteString("\t\tif _e := json.Unmarshal(_sc.Bytes(), &_req); _e != nil {\n")
	b.WriteString("\t\t\t_enc.Encode(_hResp{Error: \"bad request: \" + _e.Error()})\n") //nolint:errcheck
	b.WriteString("\t\t\tcontinue\n")
	b.WriteString("\t\t}\n\n")

	// Deserialize typed input parameters from _req.Inputs
	for i, p := range params {
		b.WriteString(fmt.Sprintf("\t\tvar %s %s\n", p.Name, p.GoType))
		b.WriteString(fmt.Sprintf("\t\tif %d < len(_req.Inputs) {\n", i))
		b.WriteString(fmt.Sprintf("\t\t\tif _e := json.Unmarshal(_req.Inputs[%d], &%s); _e != nil {\n", i, p.Name))
		b.WriteString(fmt.Sprintf("\t\t\t\t_enc.Encode(_hResp{Error: fmt.Sprintf(\"unmarshal %s: %%v\", _e)})\n", p.Name)) //nolint:errcheck
		b.WriteString("\t\t\t\tcontinue\n")
		b.WriteString("\t\t\t}\n")
		b.WriteString("\t\t}\n")
	}
	b.WriteString("\n")

	// Reset recorder state (must happen before any function call so recordings
	// from the previous iteration don't bleed into this one).
	b.WriteString("\t\t__shatter_reset()\n")
	if hasMocks {
		b.WriteString("\t\tshatterResetMockCounters()\n")
	}
	b.WriteString("\n")

	// Snapshot exported global variables before the call.
	if len(globalVars) > 0 {
		for _, v := range globalVars {
			b.WriteString(fmt.Sprintf("\t\t_bef_%s, _ok_%s := func() (json.RawMessage, bool) {\n", v.Name, v.Name))
			b.WriteString(fmt.Sprintf("\t\t\t_b, _e := json.Marshal(%s)\n", v.Name))
			b.WriteString("\t\t\treturn _b, _e == nil\n")
			b.WriteString("\t\t}()\n")
		}
		b.WriteString("\n")
	}

	// Performance counters
	b.WriteString("\t\tvar _mBef runtime.MemStats\n")
	b.WriteString("\t\truntime.ReadMemStats(&_mBef)\n")
	b.WriteString("\t\t_tStart := time.Now()\n\n")

	// Console capture: redirect os.Stdout/os.Stderr to pipes so fmt.Print* calls
	// from the target function are captured rather than mixing with JSON responses.
	b.WriteString("\t\t_rOut, _wOut, _ := os.Pipe()\n")
	b.WriteString("\t\t_origOut := os.Stdout\n")
	b.WriteString("\t\tos.Stdout = _wOut\n")
	b.WriteString("\t\tvar _capOut bytes.Buffer\n")
	b.WriteString("\t\t_donOut := make(chan struct{})\n")
	b.WriteString("\t\tgo func() { io.Copy(&_capOut, _rOut); close(_donOut) }()\n\n") //nolint:errcheck

	b.WriteString("\t\t_rErr, _wErr, _ := os.Pipe()\n")
	b.WriteString("\t\t_origErr := os.Stderr\n")
	b.WriteString("\t\tos.Stderr = _wErr\n")
	b.WriteString("\t\tvar _capErr bytes.Buffer\n")
	b.WriteString("\t\t_donErr := make(chan struct{})\n")
	b.WriteString("\t\tgo func() { io.Copy(&_capErr, _rErr); close(_donErr) }()\n\n") //nolint:errcheck

	// Declare result variable(s) before the closure so they're accessible afterwards.
	switch {
	case retInfo.Count == 1:
		b.WriteString(fmt.Sprintf("\t\tvar _res %s\n", retInfo.Types[0]))
	case retInfo.Count > 1:
		for i, t := range retInfo.Types {
			if i == retInfo.Count-1 && retInfo.HasErr {
				b.WriteString("\t\tvar _retErr error\n")
			} else {
				b.WriteString(fmt.Sprintf("\t\tvar _ret%d %s\n", i, t))
			}
		}
	}

	// Panic-recovering closure wraps the function call.
	b.WriteString("\t\tvar _thrownErr *_hError\n")
	b.WriteString("\t\tfunc() {\n")
	b.WriteString("\t\t\tdefer func() {\n")
	b.WriteString("\t\t\t\tif _r := recover(); _r != nil {\n")
	b.WriteString("\t\t\t\t\t_stk := make([]byte, 4096)\n")
	b.WriteString("\t\t\t\t\t_n := runtime.Stack(_stk, false)\n")
	b.WriteString("\t\t\t\t\t_thrownErr = &_hError{\n")
	b.WriteString("\t\t\t\t\t\tErrorType:     \"panic\",\n")
	b.WriteString("\t\t\t\t\t\tMessage:       fmt.Sprintf(\"%v\", _r),\n")
	b.WriteString("\t\t\t\t\t\tStack:         string(_stk[:_n]),\n")
	b.WriteString("\t\t\t\t\t\tErrorCategory: \"runtime\",\n")
	b.WriteString("\t\t\t\t\t}\n")
	b.WriteString("\t\t\t\t}\n")
	b.WriteString("\t\t\t}()\n")

	argList := make([]string, len(params))
	for i, p := range params {
		argList[i] = p.Name
	}
	callExpr := fmt.Sprintf("%s(%s)", funcName, strings.Join(argList, ", "))

	switch {
	case retInfo.Count == 0:
		b.WriteString(fmt.Sprintf("\t\t\t%s\n", callExpr))
	case retInfo.Count == 1:
		b.WriteString(fmt.Sprintf("\t\t\t_res = %s\n", callExpr))
	default:
		retVars := make([]string, retInfo.Count)
		for i := range retInfo.Count {
			if i == retInfo.Count-1 && retInfo.HasErr {
				retVars[i] = "_retErr"
			} else {
				retVars[i] = fmt.Sprintf("_ret%d", i)
			}
		}
		b.WriteString(fmt.Sprintf("\t\t\t%s = %s\n", strings.Join(retVars, ", "), callExpr))
	}
	b.WriteString("\t\t}()\n\n")

	// Restore stdout/stderr and drain capture pipes.
	b.WriteString("\t\tos.Stdout = _origOut\n")
	b.WriteString("\t\t_wOut.Close()\n")
	b.WriteString("\t\t<-_donOut\n")
	b.WriteString("\t\tos.Stderr = _origErr\n")
	b.WriteString("\t\t_wErr.Close()\n")
	b.WriteString("\t\t<-_donErr\n\n")

	// Performance counters (end)
	b.WriteString("\t\t_tElapsed := time.Since(_tStart)\n")
	b.WriteString("\t\tvar _mAft runtime.MemStats\n")
	b.WriteString("\t\truntime.ReadMemStats(&_mAft)\n\n")

	// Build response
	b.WriteString("\t\t_rec := __shatter_collect_results()\n")
	b.WriteString("\t\t_resp := _hResp{\n")
	b.WriteString("\t\t\tBranchPath:    _rec.BranchPath,\n")
	b.WriteString("\t\t\tLinesExecuted: _rec.LinesExecuted,\n")
	b.WriteString("\t\t\tScopeEvents:   _rec.ScopeEvents,\n")
	b.WriteString("\t\t\tThrownError:   _thrownErr,\n")
	b.WriteString("\t\t\tPerformance: &_hPerf{\n")
	b.WriteString("\t\t\t\tCPUTimeUs:          _tElapsed.Microseconds(),\n")
	b.WriteString("\t\t\t\tHeapUsedBytes:      int64(_mAft.HeapInuse) - int64(_mBef.HeapInuse),\n")
	b.WriteString("\t\t\t\tHeapAllocatedBytes: int64(_mAft.TotalAlloc) - int64(_mBef.TotalAlloc),\n")
	b.WriteString("\t\t\t},\n")
	b.WriteString("\t\t}\n\n")

	// Serialize return value
	switch {
	case retInfo.Count == 1:
		b.WriteString("\t\tif _rv, _e := json.Marshal(_res); _e == nil {\n")
		b.WriteString("\t\t\t_resp.ReturnValue = _rv\n")
		b.WriteString("\t\t}\n\n")
	case retInfo.Count > 1:
		if retInfo.HasErr {
			b.WriteString("\t\tif _retErr != nil && _thrownErr == nil {\n")
			b.WriteString("\t\t\t_resp.ThrownError = &_hError{ErrorType: \"function_error\", Message: _retErr.Error(), ErrorCategory: \"runtime\"}\n")
			b.WriteString("\t\t}\n")
		}
		nonErrCount := retInfo.Count
		if retInfo.HasErr {
			nonErrCount--
		}
		if nonErrCount == 1 {
			b.WriteString("\t\tif _rv, _e := json.Marshal(_ret0); _e == nil {\n")
			b.WriteString("\t\t\t_resp.ReturnValue = _rv\n")
			b.WriteString("\t\t}\n\n")
		} else {
			b.WriteString("\t\t_multi := []interface{}{\n")
			for i := 0; i < nonErrCount; i++ {
				b.WriteString(fmt.Sprintf("\t\t\tinterface{}(_ret%d),\n", i))
			}
			b.WriteString("\t\t}\n")
			b.WriteString("\t\tif _rv, _e := json.Marshal(_multi); _e == nil {\n")
			b.WriteString("\t\t\t_resp.ReturnValue = _rv\n")
			b.WriteString("\t\t}\n\n")
		}
	}

	// Console side effects (only included when capture=true)
	b.WriteString("\t\tif _req.Capture {\n")
	b.WriteString("\t\t\tif _s := strings.TrimSpace(_capOut.String()); _s != \"\" {\n")
	b.WriteString("\t\t\t\t_resp.SideEffects = append(_resp.SideEffects, _hSideEffect{Kind: \"console_output\", Level: \"log\", Message: _s})\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t\tif _s := strings.TrimSpace(_capErr.String()); _s != \"\" {\n")
	b.WriteString("\t\t\t\t_resp.SideEffects = append(_resp.SideEffects, _hSideEffect{Kind: \"console_output\", Level: \"error\", Message: _s})\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t}\n\n")

	// Global state changes
	if len(globalVars) > 0 {
		for _, v := range globalVars {
			b.WriteString(fmt.Sprintf("\t\tif _ok_%s {\n", v.Name))
			b.WriteString(fmt.Sprintf("\t\t\tif _aft_%s, _e := json.Marshal(%s); _e == nil {\n", v.Name, v.Name))
			b.WriteString(fmt.Sprintf("\t\t\t\tif string(_aft_%s) != string(_bef_%s) {\n", v.Name, v.Name))
			b.WriteString("\t\t\t\t\t_resp.SideEffects = append(_resp.SideEffects, _hSideEffect{\n")
			b.WriteString("\t\t\t\t\t\tKind:     \"global_state_change\",\n")
			b.WriteString(fmt.Sprintf("\t\t\t\t\t\tVariable: %q,\n", v.Name))
			b.WriteString(fmt.Sprintf("\t\t\t\t\t\tBefore:   _bef_%s,\n", v.Name))
			b.WriteString(fmt.Sprintf("\t\t\t\t\t\tAfter:    json.RawMessage(_aft_%s),\n", v.Name))
			b.WriteString("\t\t\t\t\t})\n")
			b.WriteString("\t\t\t\t}\n")
			b.WriteString("\t\t\t}\n")
			b.WriteString("\t\t}\n")
		}
		b.WriteString("\n")
	}

	// Mock call records
	if hasMocks {
		b.WriteString("\t\t_resp.ExternalCalls = shatterGetAndResetMockCalls()\n\n")
	}

	b.WriteString("\t\t_enc.Encode(_resp)\n") //nolint:errcheck
	b.WriteString("\t}\n") // end for _sc.Scan()
	b.WriteString("}\n")   // end main()

	return b.String(), nil
}

// generateLoopMockFile is the loop-harness variant of generateMockFile. It adds
// shatterResetMockCounters() and shatterGetAndResetMockCalls() so the harness can
// reset mock state between iterations and collect call records into the response.
func generateLoopMockFile(mocks []MockConfig) string {
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
	b.WriteString("\t\"sync\"\n")
	b.WriteString(")\n\n")

	b.WriteString("type shatterMockCall struct {\n")
	b.WriteString("\tSymbol      string          `json:\"symbol\"`\n")
	b.WriteString("\tArgs        json.RawMessage `json:\"args\"`\n")
	b.WriteString("\tReturnValue json.RawMessage `json:\"return_value\"`\n")
	b.WriteString("}\n\n")

	b.WriteString("var (\n")
	b.WriteString("\tshatterMockCalls   []shatterMockCall\n")
	b.WriteString("\tshatterMockCallsMu sync.Mutex\n")
	b.WriteString(")\n\n")

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

	// shatterResetMockCounters resets all per-mock call indices and the accumulated
	// call list so each loop iteration starts from a clean state.
	b.WriteString("func shatterResetMockCounters() {\n")
	for i := range mocks {
		b.WriteString(fmt.Sprintf("\tshatterMock%d_callIdx = 0\n", i))
	}
	b.WriteString("\tshatterMockCallsMu.Lock()\n")
	b.WriteString("\tshatterMockCalls = shatterMockCalls[:0]\n")
	b.WriteString("\tshatterMockCallsMu.Unlock()\n")
	b.WriteString("}\n\n")

	// shatterGetAndResetMockCalls returns the recorded calls as raw JSON and clears
	// the list so the next iteration starts fresh.
	b.WriteString("func shatterGetAndResetMockCalls() []json.RawMessage {\n")
	b.WriteString("\tshatterMockCallsMu.Lock()\n")
	b.WriteString("\tdefer shatterMockCallsMu.Unlock()\n")
	b.WriteString("\tif len(shatterMockCalls) == 0 {\n")
	b.WriteString("\t\treturn nil\n")
	b.WriteString("\t}\n")
	b.WriteString("\tout := make([]json.RawMessage, len(shatterMockCalls))\n")
	b.WriteString("\tfor i, c := range shatterMockCalls {\n")
	b.WriteString("\t\tout[i], _ = json.Marshal(c)\n")
	b.WriteString("\t}\n")
	b.WriteString("\tshatterMockCalls = shatterMockCalls[:0]\n")
	b.WriteString("\treturn out\n")
	b.WriteString("}\n\n")

	// Generate mock function variables (same logic as generateMockFile)
	for i, mock := range mocks {
		if mock.DefaultBehavior == BehaviorPassthrough {
			continue
		}

		safeName := sanitizeMockName(mock.Symbol)
		retValsJSON, _ := json.Marshal(mock.ReturnValues)

		b.WriteString(fmt.Sprintf("// Mock for %s\n", mock.Symbol))
		b.WriteString(fmt.Sprintf("var shatterMock%d_retvals = func() []json.RawMessage {\n", i))
		b.WriteString("\tvar vals []any\n")
		b.WriteString(fmt.Sprintf("\tjson.Unmarshal([]byte(`%s`), &vals)\n", string(retValsJSON)))
		b.WriteString("\tresult := make([]json.RawMessage, len(vals))\n")
		b.WriteString("\tfor i, v := range vals {\n")
		b.WriteString("\t\tresult[i], _ = json.Marshal(v)\n")
		b.WriteString("\t}\n")
		b.WriteString("\treturn result\n")
		b.WriteString("}()\n")
		b.WriteString(fmt.Sprintf("var shatterMock%d_callIdx int\n\n", i))

		if mock.DefaultBehavior == BehaviorThrowError {
			b.WriteString(fmt.Sprintf("func ShatterMock_%s(args ...any) any {\n", safeName))
			b.WriteString(fmt.Sprintf("\tretvals := shatterMock%d_retvals\n", i))
			b.WriteString(fmt.Sprintf("\tidx := shatterMock%d_callIdx\n", i))
			b.WriteString("\tif idx >= len(retvals) && len(retvals) > 0 {\n")
			b.WriteString("\t\tidx = len(retvals) - 1\n")
			b.WriteString("\t}\n")
			b.WriteString(fmt.Sprintf("\tshatterMock%d_callIdx++\n", i))
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

			b.WriteString(fmt.Sprintf("func ShatterMockErr_%s(args ...any) (any, error) {\n", safeName))
			b.WriteString(fmt.Sprintf("\tretvals := shatterMock%d_retvals\n", i))
			b.WriteString(fmt.Sprintf("\tidx := shatterMock%d_callIdx\n", i))
			b.WriteString("\tif idx >= len(retvals) && len(retvals) > 0 {\n")
			b.WriteString("\t\tidx = len(retvals) - 1\n")
			b.WriteString("\t}\n")
			b.WriteString(fmt.Sprintf("\tshatterMock%d_callIdx++\n", i))
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
