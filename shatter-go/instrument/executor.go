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

// workspaceGoEnvProvider, when non-nil, returns the environment slice to use
// for every `go build` invoked from shatter-go. Set by the protocol handler
// during construction (see protocol/handler.go) so that GOCACHE is pinned to
// the persistent workspace-backed build cache. When nil (e.g., in unit tests
// that don't wire a workspace), the helpers fall back to the legacy
// SHATTER_HARNESS_CACHE-based behavior.
var workspaceGoEnvProvider func() []string

// SetWorkspaceGoEnvProvider installs the environment provider used for `go
// build` invocations. Passing nil disables workspace-backed GOCACHE pinning
// and restores the legacy fallback.
func SetWorkspaceGoEnvProvider(fn func() []string) {
	workspaceGoEnvProvider = fn
}

// WorkspaceGoEnv returns the workspace-backed environment slice when a
// provider has been installed, or nil otherwise. Callers outside this package
// (notably setup.Loader) use the nil signal to decide whether to pin GOCACHE.
func WorkspaceGoEnv() []string {
	if workspaceGoEnvProvider == nil {
		return nil
	}
	return workspaceGoEnvProvider()
}

// applyGoBuildEnv assigns cmd.Env using the workspace-backed provider when
// set; otherwise uses the legacy per-source-kind cache directory. This is the
// single entry point for every `go build` invoked from this package.
func applyGoBuildEnv(cmd *exec.Cmd, sourcePath string) {
	if workspaceGoEnvProvider != nil {
		cmd.Env = workspaceGoEnvProvider()
		return
	}
	var gocache string
	if isStandaloneGoFile(sourcePath) {
		gocache = standaloneGoBuildCacheDir()
	} else {
		gocache = moduleGoBuildCacheDir()
	}
	if gocache != "" {
		cmd.Env = append(os.Environ(), "GOCACHE="+gocache)
	}
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

// moduleHarnessHash returns a short deterministic identifier for a module-backed
// harness keyed by source path, function name, and mock configuration. Used to
// create stable scratch subdirectory names under SHATTER_HARNESS_SCRATCH.
func moduleHarnessHash(sourcePath, funcName, mocksHash string) string {
	input := sourcePath + "\x00" + funcName + "\x00" + mocksHash
	h := sha256.Sum256([]byte(input))
	return hex.EncodeToString(h[:8]) // 16 hex chars
}

// makeModuleScratchDir creates a stable scratch directory for a module-backed
// harness build, keyed by hash. Uses SHATTER_HARNESS_SCRATCH/go/module/<hash>
// when set; falls back to os.MkdirTemp otherwise.
func makeModuleScratchDir(hash string) (string, error) {
	if scratch := harnessScratchDir(); scratch != "" {
		dir := filepath.Join(scratch, "go", "module", hash)
		if err := os.MkdirAll(dir, 0755); err == nil {
			return dir, nil
		}
		// Fall through to MkdirTemp if scratch creation fails.
	}
	return os.MkdirTemp("", "shatter-instrument-*")
}

// moduleGoBuildCacheDir returns the Go build cache path for module-backed harness
// builds. Uses SHATTER_HARNESS_CACHE/go/module/build-cache when the cache env var
// is set. Returns empty string when no cache is configured.
// The returned path is always absolute (Go requires GOCACHE to be absolute).
func moduleGoBuildCacheDir() string {
	cache := harnessCacheDir()
	if cache == "" {
		return ""
	}
	p := filepath.Join(cache, "go", "module", "build-cache")
	if !filepath.IsAbs(p) {
		if abs, err := filepath.Abs(p); err == nil {
			return abs
		}
	}
	return p
}

// harnessRuntimeModuleName is the Go module path used by the shared harness
// runtime package. Generated harness binaries import this module and the
// output directory's go.mod has a replace directive pointing at the cached
// source.
const harnessRuntimeModuleName = "shatter-harness"

// harnessRuntimeSourceCache caches the resolved path to the harness runtime
// source directory so we only write it once per process.
var (
	harnessRuntimeOnce sync.Once
	harnessRuntimeDir  string
	harnessRuntimeErr  error
)

// ensureHarnessRuntimeDir writes the harness runtime Go source to a cache
// directory and returns the absolute path. The directory contains a go.mod
// and runtime.go ready to be used as a replace target. The source is written
// once per process and reused thereafter.
func ensureHarnessRuntimeDir() (string, error) {
	harnessRuntimeOnce.Do(func() {
		var base string
		if cache := harnessCacheDir(); cache != "" {
			base = filepath.Join(cache, "go", "harness-runtime")
		} else if scratch := harnessScratchDir(); scratch != "" {
			base = filepath.Join(scratch, "go", "harness-runtime")
		} else {
			base, harnessRuntimeErr = os.MkdirTemp("", "shatter-harness-runtime-*")
			if harnessRuntimeErr != nil {
				return
			}
		}
		if err := os.MkdirAll(base, 0755); err != nil {
			harnessRuntimeErr = fmt.Errorf("creating harness runtime dir: %w", err)
			return
		}

		goMod := "module " + harnessRuntimeModuleName + "\n\ngo 1.23\n"
		if err := os.WriteFile(filepath.Join(base, "go.mod"), []byte(goMod), 0644); err != nil {
			harnessRuntimeErr = fmt.Errorf("writing harness runtime go.mod: %w", err)
			return
		}
		if err := os.WriteFile(filepath.Join(base, "runtime.go"), []byte(harnessRuntimeSource()), 0644); err != nil {
			harnessRuntimeErr = fmt.Errorf("writing harness runtime.go: %w", err)
			return
		}

		abs, err := filepath.Abs(base)
		if err != nil {
			harnessRuntimeDir = base
		} else {
			harnessRuntimeDir = abs
		}
	})
	return harnessRuntimeDir, harnessRuntimeErr
}

// appendHarnessRequire appends a require + replace directive to the go.mod in
// outputDir so the generated harness can import the shared harness runtime.
func appendHarnessRequire(outputDir, runtimeDir string) error {
	modPath := filepath.Join(outputDir, "go.mod")
	f, err := os.OpenFile(modPath, os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		return fmt.Errorf("opening go.mod for append: %w", err)
	}
	defer f.Close()

	directive := fmt.Sprintf("\nrequire %s v0.0.0\nreplace %s => %s\n",
		harnessRuntimeModuleName, harnessRuntimeModuleName, runtimeDir)
	if _, err := f.WriteString(directive); err != nil {
		return fmt.Errorf("writing harness replace directive: %w", err)
	}
	return nil
}

// copySiblingGoFiles copies all non-test .go files from the source file's
// package directory into destDir, skipping the source file itself and any
// files that already exist in destDir. This makes unexported symbols from
// sibling files available to the harness build without re-instrumenting them.
func copySiblingGoFiles(sourcePath, destDir string) error {
	srcDir := filepath.Dir(sourcePath)
	srcBase := filepath.Base(sourcePath)
	entries, err := os.ReadDir(srcDir)
	if err != nil {
		return err
	}
	for _, entry := range entries {
		name := entry.Name()
		if entry.IsDir() || filepath.Ext(name) != ".go" ||
			name == srcBase || strings.HasSuffix(name, "_test.go") {
			continue
		}
		dst := filepath.Join(destDir, name)
		if _, err := os.Stat(dst); err == nil {
			continue // already present (e.g. recorder file with same name)
		}
		data, err := os.ReadFile(filepath.Join(srcDir, name))
		if err != nil {
			return err
		}
		if err := os.WriteFile(dst, data, 0644); err != nil {
			return err
		}
	}
	return nil
}

// globalVarInfo holds the name of an exported package-level variable to track.
type globalVarInfo struct {
	Name string
}

// subprocessPackages lists Go packages that spawn external processes.
var subprocessPackages = map[string]bool{
	"os/exec": true,
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
	// Module-backed files use a semantic harness: a stable scratch dir keyed on
	// (sourcePath, funcName, mocks) under SHATTER_HARNESS_SCRATCH, with sibling
	// files copied for package-private access and GOCACHE pointed at
	// SHATTER_HARNESS_CACHE/go/module/build-cache.
	var outputDir string
	if isStandaloneGoFile(sourcePath) {
		outputDir, err = makeStandaloneScratchDir()
	} else {
		hash := moduleHarnessHash(sourcePath, funcName, computeMocksHash(activeMocks))
		outputDir, err = makeModuleScratchDir(hash)
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

	// For module-backed files, copy unexported sibling helpers so the harness build
	// can access package-private symbols. Non-fatal: if sibling copying fails we
	// attempt the build anyway (it will fail at compile time if symbols are missing).
	if !isStandaloneGoFile(sourcePath) {
		if sibErr := copySiblingGoFiles(sourcePath, outputDir); sibErr != nil {
			// Log as warning; a build failure will surface the real problem.
			fmt.Fprintf(os.Stderr, "[shatter-go] warning: copying sibling Go files: %v\n", sibErr)
		}
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

	// Wire the shared harness runtime into the generated module's go.mod.
	runtimeDir, err := ensureHarnessRuntimeDir()
	if err != nil {
		os.RemoveAll(outputDir)
		return nil, fmt.Errorf("harness runtime: %w", err)
	}
	if err := appendHarnessRequire(outputDir, runtimeDir); err != nil {
		os.RemoveAll(outputDir)
		return nil, fmt.Errorf("harness require: %w", err)
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
	applyGoBuildEnv(buildCmd, sourcePath)
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
				Performance:   PerfMetrics{WallTimeMs: float64(wallTime.Microseconds()) / 1000.0},
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
		Performance:            PerfMetrics{WallTimeMs: float64(wallTime.Microseconds()) / 1000.0},
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

// maxInterfaceStubMethods is the default per-interface method cap for
// synthesized stubs. Interfaces with more methods than this are not
// stubbed — the planner leaves them as-is and the harness falls back to
// JSON unmarshaling (which produces a nil interface and will panic on
// first method call; broader support is tracked under str-hy9b.I3).
const maxInterfaceStubMethods = 5

// paramInfo holds a parameter's name and Go type string for harness generation.
type paramInfo struct {
	Name   string
	GoType string
	// Stub is non-nil when the parameter is an interface with no concrete
	// implementation in the analyzed file and a method count at or below
	// maxInterfaceStubMethods. The harness generator emits a stub type
	// satisfying the interface instead of unmarshaling the parameter from JSON.
	Stub *interfaceStubInfo
}

// interfaceStubInfo describes a synthesized implementation of a small
// interface parameter. TypeName is the bare interface name. Methods is
// the ordered list of methods that must be implemented; each renders into
// the harness file with zero-value returns and a recorder call.
type interfaceStubInfo struct {
	TypeName string
	Methods  []stubMethodInfo
	// Imports lists package paths the harness must import so the rendered
	// method signatures and zero-value declarations type-check.
	Imports []string
}

// stubMethodInfo carries the rendered signature pieces for one method on
// a generated stub.
type stubMethodInfo struct {
	Name    string
	Params  []string // each entry is "<name> <type>"
	Returns []string // each entry is a rendered Go type
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

		// C4: Method targets cannot be invoked without a constructed receiver. Phase E
		// (invocation planning) will add that support. Until then, reject early so the
		// caller receives an unsupported outcome instead of a compile-time build failure.
		if fn.Recv != nil && len(fn.Recv.List) > 0 {
			fmt.Fprintf(os.Stderr, "[shatter-go] classify: kind=method function=%s\n", funcName)
			return nil, returnTypeInfo{}, fmt.Errorf("method target not supported: %s requires receiver planning (Phase E)", funcName)
		}
		fmt.Fprintf(os.Stderr, "[shatter-go] classify: kind=function function=%s\n", funcName)

		pkgName := file.Name.Name
		params := extractParamInfo(fn, info, pkgName, file)
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

func extractParamInfo(fn *ast.FuncDecl, info *types.Info, pkgName string, file *ast.File) []paramInfo {
	if fn.Type.Params == nil {
		return nil
	}
	var params []paramInfo
	for _, field := range fn.Type.Params.List {
		goType := resolveGoType(field.Type, info, pkgName)
		stub := interfaceStubFor(field.Type, info, file, pkgName)
		for _, name := range field.Names {
			params = append(params, paramInfo{Name: name.Name, GoType: goType, Stub: stub})
		}
	}
	return params
}

// interfaceStubFor returns a stub descriptor if expr is an interface type
// with 1 to maxInterfaceStubMethods methods and no struct declared in the
// same file implements it. Otherwise it returns nil.
func interfaceStubFor(expr ast.Expr, info *types.Info, file *ast.File, pkgName string) *interfaceStubInfo {
	if info == nil || file == nil {
		return nil
	}
	tv, ok := info.Types[expr]
	if !ok || tv.Type == nil {
		return nil
	}
	named, _ := tv.Type.(*types.Named)
	if named == nil {
		return nil
	}
	iface, ok := named.Underlying().(*types.Interface)
	if !ok {
		return nil
	}
	n := iface.NumMethods()
	if n < 1 || n > maxInterfaceStubMethods {
		return nil
	}
	if localStructImplements(file, info, iface) {
		return nil
	}
	qualifier := func(p *types.Package) string {
		if p == nil {
			return ""
		}
		if p.Name() == pkgName {
			return ""
		}
		return p.Name()
	}
	importPaths := map[string]bool{}
	collectImportPaths := func(t types.Type) {
		// Walk once — record every named type's package path.
		seen := map[types.Type]bool{}
		var walk func(types.Type)
		walk = func(t types.Type) {
			if t == nil || seen[t] {
				return
			}
			seen[t] = true
			switch v := t.(type) {
			case *types.Named:
				if obj := v.Obj(); obj != nil && obj.Pkg() != nil && obj.Pkg().Name() != pkgName {
					importPaths[obj.Pkg().Path()] = true
				}
				walk(v.Underlying())
			case *types.Pointer:
				walk(v.Elem())
			case *types.Slice:
				walk(v.Elem())
			case *types.Array:
				walk(v.Elem())
			case *types.Map:
				walk(v.Key())
				walk(v.Elem())
			case *types.Chan:
				walk(v.Elem())
			case *types.Signature:
				if p := v.Params(); p != nil {
					for i := 0; i < p.Len(); i++ {
						walk(p.At(i).Type())
					}
				}
				if r := v.Results(); r != nil {
					for i := 0; i < r.Len(); i++ {
						walk(r.At(i).Type())
					}
				}
			}
		}
		walk(t)
	}
	methods := make([]stubMethodInfo, 0, n)
	for i := range n {
		m := iface.Method(i)
		sig, ok := m.Type().(*types.Signature)
		if !ok {
			return nil
		}
		var sm stubMethodInfo
		sm.Name = m.Name()
		if p := sig.Params(); p != nil {
			for j := 0; j < p.Len(); j++ {
				pv := p.At(j)
				collectImportPaths(pv.Type())
				name := pv.Name()
				if name == "" {
					name = fmt.Sprintf("_p%d", j)
				}
				sm.Params = append(sm.Params, name+" "+types.TypeString(pv.Type(), qualifier))
			}
		}
		if r := sig.Results(); r != nil {
			for j := 0; j < r.Len(); j++ {
				rv := r.At(j)
				collectImportPaths(rv.Type())
				sm.Returns = append(sm.Returns, types.TypeString(rv.Type(), qualifier))
			}
		}
		methods = append(methods, sm)
	}
	imports := make([]string, 0, len(importPaths))
	for p := range importPaths {
		imports = append(imports, p)
	}
	sort.Strings(imports)
	return &interfaceStubInfo{
		TypeName: named.Obj().Name(),
		Methods:  methods,
		Imports:  imports,
	}
}

// collectStubParams returns the subset of params that need a generated
// interface stub, preserving parameter order.
func collectStubParams(params []paramInfo) []paramInfo {
	var out []paramInfo
	for _, p := range params {
		if p.Stub != nil {
			out = append(out, p)
		}
	}
	return out
}

// extraStubImports returns the deduplicated sorted list of package paths
// the harness needs to import so stub method signatures type-check.
// Packages already imported unconditionally by the harness (encoding/json,
// fmt, sync) are excluded.
func extraStubImports(stubParams []paramInfo) []string {
	baseline := map[string]bool{
		"encoding/json": true,
		"fmt":           true,
		"sync":          true,
	}
	seen := map[string]bool{}
	var out []string
	for _, p := range stubParams {
		for _, imp := range p.Stub.Imports {
			if baseline[imp] || seen[imp] {
				continue
			}
			seen[imp] = true
			out = append(out, imp)
		}
	}
	sort.Strings(out)
	return out
}

// writeStubDeclarations emits the stub recorder type + mutex + helper,
// followed by one struct + method block per stubbed param. Each method
// records a call into shatterStubCalls and returns zero values.
func writeStubDeclarations(b *strings.Builder, stubParams []paramInfo) {
	b.WriteString("type shatterStubCall struct {\n")
	b.WriteString("\tType   string\n")
	b.WriteString("\tMethod string\n")
	b.WriteString("}\n\n")
	b.WriteString("var shatterStubCalls []shatterStubCall\n")
	b.WriteString("var shatterStubCallsMu sync.Mutex\n\n")
	b.WriteString("func shatterRecordStubCall(t, m string) {\n")
	b.WriteString("\tshatterStubCallsMu.Lock()\n")
	b.WriteString("\tdefer shatterStubCallsMu.Unlock()\n")
	b.WriteString("\tshatterStubCalls = append(shatterStubCalls, shatterStubCall{Type: t, Method: m})\n")
	b.WriteString("}\n\n")
	// Deduplicate by TypeName in case the same interface appears twice.
	emitted := map[string]bool{}
	for _, p := range stubParams {
		stub := p.Stub
		if emitted[stub.TypeName] {
			continue
		}
		emitted[stub.TypeName] = true
		fmt.Fprintf(b, "type shatterStub_%s struct{}\n\n", stub.TypeName)
		for _, m := range stub.Methods {
			fmt.Fprintf(b, "func (s *shatterStub_%s) %s(%s)", stub.TypeName, m.Name, strings.Join(m.Params, ", "))
			switch len(m.Returns) {
			case 0:
				b.WriteString(" {\n")
			case 1:
				b.WriteString(" " + m.Returns[0] + " {\n")
			default:
				b.WriteString(" (" + strings.Join(m.Returns, ", ") + ") {\n")
			}
			fmt.Fprintf(b, "\tshatterRecordStubCall(%q, %q)\n", stub.TypeName, m.Name)
			if len(m.Returns) == 0 {
				b.WriteString("}\n\n")
				continue
			}
			retNames := make([]string, len(m.Returns))
			for i, r := range m.Returns {
				retNames[i] = fmt.Sprintf("_r%d", i)
				fmt.Fprintf(b, "\tvar _r%d %s\n", i, r)
			}
			b.WriteString("\treturn " + strings.Join(retNames, ", ") + "\n")
			b.WriteString("}\n\n")
		}
	}
}

// localStructImplements reports whether any struct type declared in the
// same file implements iface (checked with both value and pointer
// receivers). Mirrors the pattern in protocol/analyzer.go.
func localStructImplements(file *ast.File, info *types.Info, iface *types.Interface) bool {
	for _, decl := range file.Decls {
		gd, ok := decl.(*ast.GenDecl)
		if !ok || gd.Tok != token.TYPE {
			continue
		}
		for _, spec := range gd.Specs {
			ts, ok := spec.(*ast.TypeSpec)
			if !ok {
				continue
			}
			obj, ok := info.Defs[ts.Name].(*types.TypeName)
			if !ok || obj == nil {
				continue
			}
			named, ok := obj.Type().(*types.Named)
			if !ok {
				continue
			}
			if _, isStruct := named.Underlying().(*types.Struct); !isStruct {
				continue
			}
			if types.Implements(named, iface) ||
				types.Implements(types.NewPointer(named), iface) {
				return true
			}
		}
	}
	return false
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

	stubParams := collectStubParams(params)
	extraImports := extraStubImports(stubParams)

	b.WriteString("package main\n\n")
	b.WriteString("import (\n")
	b.WriteString("\t\"encoding/json\"\n")
	if len(params) > 0 {
		b.WriteString("\t\"fmt\"\n")
	}
	if len(stubParams) > 0 {
		b.WriteString("\t\"sync\"\n")
	}
	for _, p := range extraImports {
		fmt.Fprintf(&b, "\t%q\n", p)
	}
	b.WriteString("\n")
	b.WriteString("\t\"shatter-harness\"\n")
	b.WriteString(")\n\n")

	// Stub type declarations + recorder (top-level, shared across RunLoop iterations).
	if len(stubParams) > 0 {
		writeStubDeclarations(&b, stubParams)
	}

	b.WriteString("func main() {\n")
	b.WriteString("\tharness.RunLoop(func(_req harness.Request) harness.Response {\n")

	// Deserialize typed input parameters from _req.Inputs.
	// Stubbed interface params are constructed in-process instead of
	// unmarshaled — the orchestrator cannot produce a concrete value for
	// an interface, and a nil interface would panic on first method call.
	for i, p := range params {
		if p.Stub != nil {
			fmt.Fprintf(&b, "\t\tvar %s %s = &shatterStub_%s{}\n", p.Name, p.GoType, p.Stub.TypeName)
			fmt.Fprintf(&b, "\t\t_ = %s\n", p.Name)
			continue
		}
		fmt.Fprintf(&b, "\t\tvar %s %s\n", p.Name, p.GoType)
		fmt.Fprintf(&b, "\t\tif %d < len(_req.Inputs) {\n", i)
		fmt.Fprintf(&b, "\t\t\tif _e := json.Unmarshal(_req.Inputs[%d], &%s); _e != nil {\n", i, p.Name)
		fmt.Fprintf(&b, "\t\t\t\treturn harness.Response{Error: fmt.Sprintf(\"unmarshal %s: %%v\", _e)}\n", p.Name)
		b.WriteString("\t\t\t}\n")
		b.WriteString("\t\t}\n")
	}
	b.WriteString("\n")

	// Reset recorder state
	b.WriteString("\t\t__shatter_reset()\n")
	if hasMocks {
		b.WriteString("\t\tshatterResetMockCounters()\n")
	}
	b.WriteString("\n")

	// Snapshot exported global variables before the call.
	if len(globalVars) > 0 {
		for _, v := range globalVars {
			fmt.Fprintf(&b, "\t\t_bef_%s, _ok_%s := func() (json.RawMessage, bool) {\n", v.Name, v.Name)
			fmt.Fprintf(&b, "\t\t\t_b, _e := json.Marshal(%s)\n", v.Name)
			b.WriteString("\t\t\treturn _b, _e == nil\n")
			b.WriteString("\t\t}()\n")
		}
		b.WriteString("\n")
	}

	// Performance + console capture via harness package
	b.WriteString("\t\t_perf := harness.StartPerf()\n")
	b.WriteString("\t\t_cap := harness.CaptureConsole()\n\n")

	// Declare result variable(s) before the closure so they're accessible afterwards.
	switch {
	case retInfo.Count == 1:
		fmt.Fprintf(&b, "\t\tvar _res %s\n", retInfo.Types[0])
	case retInfo.Count > 1:
		for i, t := range retInfo.Types {
			if i == retInfo.Count-1 && retInfo.HasErr {
				b.WriteString("\t\tvar _retErr error\n")
			} else {
				fmt.Fprintf(&b, "\t\tvar _ret%d %s\n", i, t)
			}
		}
	}

	// Panic-recovering call via harness.SafeCall
	argList := make([]string, len(params))
	for i, p := range params {
		argList[i] = p.Name
	}
	callExpr := fmt.Sprintf("%s(%s)", funcName, strings.Join(argList, ", "))

	b.WriteString("\t\t_thrownErr := harness.SafeCall(func() {\n")
	switch retInfo.Count {
	case 0:
		fmt.Fprintf(&b, "\t\t\t%s\n", callExpr)
	case 1:
		fmt.Fprintf(&b, "\t\t\t_res = %s\n", callExpr)
	default:
		retVars := make([]string, retInfo.Count)
		for i := range retInfo.Count {
			if i == retInfo.Count-1 && retInfo.HasErr {
				retVars[i] = "_retErr"
			} else {
				retVars[i] = fmt.Sprintf("_ret%d", i)
			}
		}
		fmt.Fprintf(&b, "\t\t\t%s = %s\n", strings.Join(retVars, ", "), callExpr)
	}
	b.WriteString("\t\t})\n\n")

	// Stop capture and finish perf measurement
	b.WriteString("\t\t_stdout, _stderr := _cap.Stop()\n")
	b.WriteString("\t\t_perfResult := _perf.Finish()\n\n")

	// Marshal recorder results to json.RawMessage
	b.WriteString("\t\t_rec := __shatter_collect_results()\n")
	b.WriteString("\t\t_branchPath, _ := json.Marshal(_rec.BranchPath)\n")
	b.WriteString("\t\t_linesExec, _ := json.Marshal(_rec.LinesExecuted)\n")
	b.WriteString("\t\t_scopeEvts, _ := json.Marshal(_rec.ScopeEvents)\n\n")

	// Build response
	b.WriteString("\t\t_resp := harness.Response{\n")
	b.WriteString("\t\t\tBranchPath:    _branchPath,\n")
	b.WriteString("\t\t\tLinesExecuted: _linesExec,\n")
	b.WriteString("\t\t\tScopeEvents:   _scopeEvts,\n")
	b.WriteString("\t\t\tThrownError:   _thrownErr,\n")
	b.WriteString("\t\t\tPerformance:   _perfResult,\n")
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
			b.WriteString("\t\t\t_resp.ThrownError = &harness.Error{ErrorType: \"function_error\", Message: _retErr.Error(), ErrorCategory: \"runtime\"}\n")
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
				fmt.Fprintf(&b, "\t\t\tinterface{}(_ret%d),\n", i)
			}
			b.WriteString("\t\t}\n")
			b.WriteString("\t\tif _rv, _e := json.Marshal(_multi); _e == nil {\n")
			b.WriteString("\t\t\t_resp.ReturnValue = _rv\n")
			b.WriteString("\t\t}\n\n")
		}
	}

	// Console side effects (only included when capture=true)
	b.WriteString("\t\tif _req.Capture {\n")
	b.WriteString("\t\t\t_resp.SideEffects = append(_resp.SideEffects, harness.ConsoleSideEffects(_stdout, _stderr)...)\n")
	b.WriteString("\t\t}\n\n")

	// Global state changes
	if len(globalVars) > 0 {
		for _, v := range globalVars {
			fmt.Fprintf(&b, "\t\tif _ok_%s {\n", v.Name)
			fmt.Fprintf(&b, "\t\t\tif _aft_%s, _e := json.Marshal(%s); _e == nil {\n", v.Name, v.Name)
			fmt.Fprintf(&b, "\t\t\t\tif string(_aft_%s) != string(_bef_%s) {\n", v.Name, v.Name)
			b.WriteString("\t\t\t\t\t_resp.SideEffects = append(_resp.SideEffects, harness.SideEffect{\n")
			b.WriteString("\t\t\t\t\t\tKind:     \"global_state_change\",\n")
			fmt.Fprintf(&b, "\t\t\t\t\t\tVariable: %q,\n", v.Name)
			fmt.Fprintf(&b, "\t\t\t\t\t\tBefore:   _bef_%s,\n", v.Name)
			fmt.Fprintf(&b, "\t\t\t\t\t\tAfter:    json.RawMessage(_aft_%s),\n", v.Name)
			b.WriteString("\t\t\t\t\t})\n")
			b.WriteString("\t\t\t\t}\n")
			b.WriteString("\t\t\t}\n")
			b.WriteString("\t\t}\n")
		}
		b.WriteString("\n")
	}

	// Mock call records
	if hasMocks {
		b.WriteString("\t\tif _mockCalls := shatterGetAndResetMockCalls(); len(_mockCalls) > 0 {\n")
		b.WriteString("\t\t\t_resp.ExternalCalls, _ = json.Marshal(_mockCalls)\n")
		b.WriteString("\t\t}\n\n")
	}

	b.WriteString("\t\treturn _resp\n")
	b.WriteString("\t})\n") // end RunLoop handler
	b.WriteString("}\n")    // end main()

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
	b.WriteString("\t\"sync/atomic\"\n")
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
		fmt.Fprintf(&b, "\tatomic.StoreInt64(&shatterMock%d_callIdx, 0)\n", i)
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

		fmt.Fprintf(&b, "// Mock for %s\n", mock.Symbol)
		fmt.Fprintf(&b, "var shatterMock%d_retvals = func() []json.RawMessage {\n", i)
		b.WriteString("\tvar vals []any\n")
		fmt.Fprintf(&b, "\tjson.Unmarshal([]byte(`%s`), &vals)\n", string(retValsJSON))
		b.WriteString("\tresult := make([]json.RawMessage, len(vals))\n")
		b.WriteString("\tfor i, v := range vals {\n")
		b.WriteString("\t\tresult[i], _ = json.Marshal(v)\n")
		b.WriteString("\t}\n")
		b.WriteString("\treturn result\n")
		b.WriteString("}()\n")
		fmt.Fprintf(&b, "var shatterMock%d_callIdx int64\n\n", i)

		if mock.DefaultBehavior == BehaviorThrowError {
			fmt.Fprintf(&b, "func ShatterMock_%s(args ...any) any {\n", safeName)
			fmt.Fprintf(&b, "\tretvals := shatterMock%d_retvals\n", i)
			fmt.Fprintf(&b, "\tidx := int(atomic.AddInt64(&shatterMock%d_callIdx, 1)) - 1\n", i)
			b.WriteString("\tif idx >= len(retvals) && len(retvals) > 0 {\n")
			b.WriteString("\t\tidx = len(retvals) - 1\n")
			b.WriteString("\t}\n")
			fmt.Fprintf(&b, "\tmsg := %q\n", MockErrorPrefix+mock.Symbol)
			b.WriteString("\tif idx < len(retvals) {\n")
			b.WriteString("\t\tvar obj map[string]any\n")
			b.WriteString("\t\tif json.Unmarshal(retvals[idx], &obj) == nil {\n")
			b.WriteString("\t\t\tif s, ok := obj[\"message\"].(string); ok && s != \"\" {\n")
			b.WriteString("\t\t\t\tmsg = s\n")
			b.WriteString("\t\t\t}\n")
			b.WriteString("\t\t}\n")
			b.WriteString("\t}\n")
			if mock.ShouldTrackCalls {
				fmt.Fprintf(&b, "\tshatterRecordMockCall(%q, args, msg)\n", mock.Symbol)
			}
			b.WriteString("\tpanic(msg)\n")
			b.WriteString("}\n\n")

			fmt.Fprintf(&b, "func ShatterMockErr_%s(args ...any) (any, error) {\n", safeName)
			fmt.Fprintf(&b, "\tretvals := shatterMock%d_retvals\n", i)
			fmt.Fprintf(&b, "\tidx := int(atomic.AddInt64(&shatterMock%d_callIdx, 1)) - 1\n", i)
			b.WriteString("\tif idx >= len(retvals) && len(retvals) > 0 {\n")
			b.WriteString("\t\tidx = len(retvals) - 1\n")
			b.WriteString("\t}\n")
			fmt.Fprintf(&b, "\tmsg := %q\n", MockErrorPrefix+mock.Symbol)
			b.WriteString("\tif idx < len(retvals) {\n")
			b.WriteString("\t\tvar obj map[string]any\n")
			b.WriteString("\t\tif json.Unmarshal(retvals[idx], &obj) == nil {\n")
			b.WriteString("\t\t\tif s, ok := obj[\"message\"].(string); ok && s != \"\" {\n")
			b.WriteString("\t\t\t\tmsg = s\n")
			b.WriteString("\t\t\t}\n")
			b.WriteString("\t\t}\n")
			b.WriteString("\t}\n")
			if mock.ShouldTrackCalls {
				fmt.Fprintf(&b, "\tshatterRecordMockCall(%q, args, msg)\n", mock.Symbol)
			}
			b.WriteString("\treturn nil, fmt.Errorf(\"%s\", msg)\n")
			b.WriteString("}\n\n")
			continue
		}

		fmt.Fprintf(&b, "func ShatterMock_%s(args ...any) any {\n", safeName)
		fmt.Fprintf(&b, "\tretvals := shatterMock%d_retvals\n", i)
		fmt.Fprintf(&b, "\tidx := int(atomic.AddInt64(&shatterMock%d_callIdx, 1)) - 1\n", i)
		if mock.DefaultBehavior == BehaviorRepeatLast || mock.DefaultBehavior == "" {
			b.WriteString("\tif idx >= len(retvals) && len(retvals) > 0 {\n")
			b.WriteString("\t\tidx = len(retvals) - 1\n")
			b.WriteString("\t}\n")
		} else {
			b.WriteString("\tif len(retvals) > 0 {\n")
			b.WriteString("\t\tidx = idx % len(retvals)\n")
			b.WriteString("\t}\n")
		}
		b.WriteString("\tvar retVal any\n")
		b.WriteString("\tif idx < len(retvals) {\n")
		b.WriteString("\t\tjson.Unmarshal(retvals[idx], &retVal)\n")
		b.WriteString("\t}\n")
		if mock.ShouldTrackCalls {
			fmt.Fprintf(&b, "\tshatterRecordMockCall(%q, args, retVal)\n", mock.Symbol)
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
