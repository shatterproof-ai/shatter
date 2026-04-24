package protocol

import (
	"bufio"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"os"
	"sort"
	"strconv"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/config"
	"github.com/shatter-dev/shatter/shatter-go/generators"
	"github.com/shatter-dev/shatter/shatter-go/instrument"
	goloader "github.com/shatter-dev/shatter/shatter-go/loader"
	"github.com/shatter-dev/shatter/shatter-go/setup"
	frontendtiming "github.com/shatter-dev/shatter/shatter-go/timing"
	"github.com/shatter-dev/shatter/shatter-go/workspace"
)

const frontendVersion = "0.1.0"
const frontendLanguage = "go"

// Handler processes protocol requests and writes responses.
type Handler struct {
	reader            *bufio.Scanner
	writer            io.Writer
	log               *slog.Logger
	lastAnalyzedFile  string // remembered from the most recent analyze command
	registry          *generators.Registry
	setupLoader       *setup.Loader
	timingEnabled     bool
	workspace         *workspace.Workspace
	loader            *goloader.Loader // lazy: built from workspace on first analyze call
	preparedHarnesses map[string]preparedExecution
	preparedTargets   map[string]string // "file\x00function" → current prepare_id for stale detection
	hookFactories     []RuntimeHookFactory
	cachedAnalyses    map[string]*FunctionAnalysis // "file\x00function" → cached analysis
	// planRequirements, when non-nil, is dispatched from handleGetInvocationPlan.
	// Injected at construction time by callers that link the planner package;
	// keeping it a function pointer avoids a protocol→planner import cycle.
	planRequirements PlannerFunc

	// policyConfigLoader returns the parsed .shatter/config.yaml nearest to
	// the given source file. Injectable so tests can supply a synthetic
	// config without touching the real filesystem. Nil defers to config.Load.
	policyConfigLoader func(file string) (config.File, error)
}

// NewHandler creates a handler reading from r, writing responses to w,
// and logging to logw at the level set by SHATTER_LOG_LEVEL.
func NewHandler(r io.Reader, w io.Writer, logw io.Writer) *Handler {
	return newHandler(r, w, logw, slogLevelFromEnv(), nil)
}

// NewHandlerWithWorkspace creates a handler with an initialized workspace.
func NewHandlerWithWorkspace(r io.Reader, w io.Writer, logw io.Writer, workspace *workspace.Workspace) *Handler {
	return newHandler(r, w, logw, slogLevelFromEnv(), workspace)
}

func newHandler(r io.Reader, w io.Writer, logw io.Writer, level slog.Level, workspace *workspace.Workspace) *Handler {
	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 0, 1024*1024), 10*1024*1024) // 10MB max line
	h := &Handler{
		reader:            scanner,
		writer:            w,
		log:               slog.New(newPrefixHandler(logw, level)),
		workspace:         workspace,
		registry:          generators.NewRegistry(),
		setupLoader:       setup.NewLoader(),
		preparedHarnesses: make(map[string]preparedExecution),
		preparedTargets:   make(map[string]string),
		cachedAnalyses:    make(map[string]*FunctionAnalysis),
	}
	// Register built-in adapter factories.
	h.RegisterHookFactory(createHTTPHandlerFactory())
	h.RegisterHookFactory(createGinHandlerFactory())
	// Pin every `go build` invoked from this process to the workspace-backed
	// GOCACHE so consecutive runs reuse compiled artifacts (str-hy9b.B2).
	if workspace != nil {
		ws := workspace
		instrument.SetWorkspaceGoEnvProvider(ws.GoEnv)
	}
	return h
}

// PlannerFunc services get_invocation_plan requests. Callers wire in the real
// planner via RegisterPlanner; keeping it a function pointer avoids a
// protocol→planner import cycle. The lookup closure resolves a target_id to
// its cached FunctionAnalysis or nil when the target was not analyzed.
type PlannerFunc func(
	requirements []InvocationRequirement,
	lookup func(targetID string) *FunctionAnalysis,
) (plans []InvocationPlan, unsatisfied []UnsatisfiedRequirement)

// RegisterPlanner installs a PlannerFunc. Passing nil clears any previously
// registered planner; unregistered handlers reply with ErrNotSupported on
// get_invocation_plan.
func (h *Handler) RegisterPlanner(fn PlannerFunc) {
	h.planRequirements = fn
}

// NewHandlerWithLogLevel creates a handler with an explicit log level (for testing).
func NewHandlerWithLogLevel(r io.Reader, w io.Writer, logw io.Writer, level string) *Handler {
	return newHandler(r, w, logw, slogLevelFromString(level), nil)
}

// Run processes requests until shutdown or EOF. Returns nil on clean shutdown.
func (h *Handler) Run() error {
	h.log.Debug("Starting Go frontend", "protocol", ProtocolVersion)

	for h.reader.Scan() {
		line := h.reader.Text()
		if line == "" {
			continue
		}

		h.log.Log(context.Background(), LevelTrace, "Received", "raw", line)

		var req Request
		if err := json.Unmarshal([]byte(line), &req); err != nil {
			h.log.Log(context.Background(), LevelTrace, "Failed to parse request", "err", err)
			errResp := Response{
				ProtocolVersion: ProtocolVersion,
				ID:              0,
				Status:          "error",
				Code:            ErrInvalidRequest,
				Message:         fmt.Sprintf("Invalid JSON: %s", err.Error()),
			}
			if sendErr := h.send(errResp); sendErr != nil {
				return fmt.Errorf("writing error response: %w", sendErr)
			}
			continue
		}

		resp, shutdown := h.dispatch(req)
		if err := h.send(resp); err != nil {
			return fmt.Errorf("writing response: %w", err)
		}

		if shutdown {
			h.log.Debug("Shutting down")
			return nil
		}
	}

	if err := h.reader.Err(); err != nil {
		return fmt.Errorf("reading stdin: %w", err)
	}

	h.log.Debug("Stdin closed, exiting")
	return nil
}

func (h *Handler) dispatch(req Request) (Response, bool) {
	base := Response{
		ProtocolVersion: ProtocolVersion,
		ID:              req.ID,
	}

	if !isVersionCompatible(req.ProtocolVersion) {
		base.Status = "error"
		base.Code = ErrVersionMismatch
		base.Message = fmt.Sprintf(
			"unsupported protocol version %q, expected %q",
			req.ProtocolVersion, ProtocolVersion,
		)
		return base, false
	}

	switch req.Command {
	case "handshake":
		return h.handleHandshake(base, req), false
	case "analyze":
		return h.handleAnalyze(base, req), false
	case "instrument":
		return h.handleInstrument(base, req), false
	case "prepare":
		return h.handlePrepare(base, req), false
	case "execute":
		return h.handleExecute(base, req), false
	case "setup":
		return h.handleSetup(base, req), false
	case "teardown":
		return h.handleTeardown(base, req), false
	case "generate":
		return h.handleGenerate(base, req), false
	case "get_invocation_plan":
		return h.handleGetInvocationPlan(base, req), false
	case "shutdown":
		return h.handleShutdown(base), true
	default:
		base.Status = "error"
		base.Code = ErrInvalidRequest
		base.Message = fmt.Sprintf("unknown command: %s", req.Command)
		return base, false
	}
}

func (h *Handler) handleHandshake(resp Response, req Request) Response {
	h.timingEnabled = hasCapability(req.Capabilities, "timing")
	resp.Status = "handshake"
	resp.FrontendVersion = frontendVersion
	resp.Language = frontendLanguage
	caps := make([]string, len(CommandCapabilities))
	copy(caps, CommandCapabilities)
	caps = append(caps,
		"complex_type:date", "complex_type:duration", "complex_type:url",
		"complex_type:reg_exp", "complex_type:ip_address", "complex_type:big_int",
		"complex_type:rational", "complex_type:big_decimal", "complex_type:error",
	)
	resp.Capabilities = caps
	return resp
}

func hasCapability(capabilities []string, want string) bool {
	for _, capability := range capabilities {
		if capability == want {
			return true
		}
	}
	return false
}

func (h *Handler) maybeTimingCollector() *frontendtiming.Collector {
	if !h.timingEnabled {
		return nil
	}
	return frontendtiming.NewCollector()
}

func finalizeResponse(resp Response, timing *frontendtiming.Collector) Response {
	if timing == nil {
		return resp
	}

	finishSerialize := timing.Start("serialize.response")
	finishSerialize()
	if summary := timing.Summary(); summary != nil {
		resp.Timing = &TimingSummary{Phases: summary.Phases}
	}
	return resp
}

// isTestFile reports whether the given file path ends in _test.go.
// Go test files cause stack overflows during type checking due to recursive
// types in the testing package, and are not valid targets for exploration.
func isTestFile(path string) bool {
	return strings.HasSuffix(path, "_test.go")
}

func (h *Handler) handleAnalyze(resp Response, req Request) Response {
	timing := h.maybeTimingCollector()
	if req.File == "" {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = "analyze command requires a file path"
		return resp
	}

	if isTestFile(req.File) {
		resp.Status = "error"
		resp.Code = ErrNotSupported
		resp.Message = fmt.Sprintf("_test.go files are not supported targets: %s", req.File)
		return resp
	}

	if _, err := os.Stat(req.File); err != nil {
		resp.Status = "error"
		resp.Code = ErrFileNotFound
		resp.Message = fmt.Sprintf("file not found: %s", req.File)
		return resp
	}

	h.lastAnalyzedFile = req.File

	var functionName string
	if req.Function != nil {
		functionName = *req.Function
	}

	finishAnalyze := timing.Start("analyze.total")
	functions, err := h.analyzeFile(req.File, functionName, timing)
	finishAnalyze()
	if err != nil {
		if functionName != "" && isNotFound(err) {
			resp.Status = "error"
			resp.Code = ErrFunctionNotFound
			resp.Message = fmt.Sprintf("function %q not found in %s", functionName, req.File)
			return resp
		}
		resp.Status = "error"
		resp.Code = ErrParseError
		resp.Message = err.Error()
		return resp
	}

	resp.Status = "analyze"
	if functions == nil {
		functions = []FunctionAnalysis{}
	}

	// Cache analysis records so execute can read invocation_model and
	// decide whether to dispatch through an adapter-owned hook.
	for i := range functions {
		key := req.File + "\x00" + functions[i].Name
		h.cachedAnalyses[key] = &functions[i]
	}

	resp.Functions = functions
	return finalizeResponse(resp, timing)
}

func isNotFound(err error) bool {
	return err != nil && strings.HasPrefix(err.Error(), "function not found")
}

// analyzeFile runs analysis via the handler's loader when a workspace is
// available; otherwise falls back to the transient-loader entry point so
// test handlers constructed without a workspace keep working.
func (h *Handler) analyzeFile(filePath string, functionName string, timing *frontendtiming.Collector) ([]FunctionAnalysis, error) {
	if h.workspace == nil {
		return AnalyzeFileWithTiming(filePath, functionName, timing)
	}
	if h.loader == nil {
		ldr, err := goloader.New(h.workspace)
		if err != nil {
			return nil, fmt.Errorf("construct analyzer loader: %w", err)
		}
		h.loader = ldr
	}
	return AnalyzeFileWithLoaderAndTiming(filePath, functionName, h.loader, timing)
}

func (h *Handler) handleInstrument(resp Response, req Request) Response {
	timing := h.maybeTimingCollector()
	if req.File == "" {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = "instrument command requires a file path"
		return resp
	}

	if isTestFile(req.File) {
		resp.Status = "error"
		resp.Code = ErrNotSupported
		resp.Message = fmt.Sprintf("_test.go files are not supported targets: %s", req.File)
		return resp
	}

	if _, err := os.Stat(req.File); err != nil {
		resp.Status = "error"
		resp.Code = ErrFileNotFound
		resp.Message = fmt.Sprintf("file not found: %s", req.File)
		return resp
	}

	h.lastAnalyzedFile = req.File

	finishInstrument := timing.Start("instrument.total")
	outputDir, err := instrument.InstrumentFileWithTiming(req.File, req.Function, req.ProjectRoot, timing)
	finishInstrument()
	if err != nil {
		resp.Status = "error"
		resp.Code = ErrInternalError
		resp.Message = fmt.Sprintf("instrumentation failed: %v", err)
		return resp
	}

	instrumented := true
	resp.Status = "instrument"
	resp.Instrumented = &instrumented
	resp.OutputFile = &outputDir
	return finalizeResponse(resp, timing)
}

// computePrepareID returns a deterministic 16-hex-char ID derived from the
// file path, function name, and sorted mock symbols.
func computePrepareID(file, function string, mocks []instrument.MockConfig) string {
	h := sha256.New()
	fmt.Fprintf(h, "%s\x00%s\x00", file, function)
	symbols := make([]string, len(mocks))
	for i, m := range mocks {
		symbols[i] = m.Symbol
	}
	sort.Strings(symbols)
	for _, s := range symbols {
		fmt.Fprintf(h, "%s\x00", s)
	}
	return hex.EncodeToString(h.Sum(nil))[:16]
}

func (h *Handler) handlePrepare(resp Response, req Request) Response {
	timing := h.maybeTimingCollector()
	file := req.File
	if file == "" {
		file = h.lastAnalyzedFile
	}
	if file == "" {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = "prepare command requires a file path (or a prior analyze)"
		return resp
	}
	if isTestFile(file) {
		resp.Status = "error"
		resp.Code = ErrNotSupported
		resp.Message = fmt.Sprintf("_test.go files are not supported targets: %s", file)
		return resp
	}
	if req.Function == nil || *req.Function == "" {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = "prepare command requires a function name"
		return resp
	}
	if _, err := os.Stat(file); err != nil {
		resp.Status = "error"
		resp.Code = ErrFileNotFound
		resp.Message = fmt.Sprintf("file not found: %s", file)
		return resp
	}

	var execMocks []instrument.MockConfig
	for _, m := range req.Mocks {
		execMocks = append(execMocks, instrument.MockConfig{
			Symbol:           m.Symbol,
			ReturnValues:     m.ReturnValues,
			ShouldTrackCalls: m.ShouldTrackCalls,
			DefaultBehavior:  m.DefaultBehavior,
		})
	}

	prepareID := computePrepareID(file, *req.Function, execMocks)
	targetKey := file + "\x00" + *req.Function

	// Invalidate stale harness if the same target was prepared with different inputs.
	if oldID, exists := h.preparedTargets[targetKey]; exists && oldID != prepareID {
		h.log.Debug("invalidating stale prepared harness", "old_prepare_id", oldID, "new_prepare_id", prepareID)
		if oldHarness, ok := h.preparedHarnesses[oldID]; ok {
			oldHarness.Cleanup()
			delete(h.preparedHarnesses, oldID)
		}
		delete(h.preparedTargets, targetKey)
	}

	// Idempotent: return immediately if already prepared and still valid.
	if existing, exists := h.preparedHarnesses[prepareID]; exists {
		if existing.IsValid() {
			h.log.Debug("prepare cache hit", "prepare_id", prepareID)
			resp.Status = "prepare"
			resp.PrepareID = prepareID
			return finalizeResponse(resp, timing)
		}
		existing.Cleanup()
		delete(h.preparedHarnesses, prepareID)
		delete(h.preparedTargets, targetKey)
	}

	h.log.Debug("Preparing harness", "file", file, "function", *req.Function, "prepare_id", prepareID)

	finishPrepare := timing.Start("prepare.total")
	harness, err := h.prepareDirectExecution(file, *req.Function, execMocks, timing, "prepare")
	finishPrepare()
	if err != nil {
		resp.Status = "error"
		if strings.Contains(err.Error(), "function not found") {
			resp.Code = ErrFunctionNotFound
		} else if strings.Contains(err.Error(), "receiver planning") {
			resp.Code = ErrNotSupported
		} else if strings.Contains(err.Error(), "build failed") {
			resp.Code = ErrInstrumentationFailed
		} else {
			resp.Code = ErrInternalError
		}
		resp.Message = err.Error()
		return resp
	}

	h.preparedHarnesses[prepareID] = harness
	h.preparedTargets[targetKey] = prepareID
	resp.Status = "prepare"
	resp.PrepareID = prepareID
	return finalizeResponse(resp, timing)
}

func (h *Handler) handleExecute(resp Response, req Request) Response {
	timing := h.maybeTimingCollector()
	file := req.File
	if file == "" {
		file = h.lastAnalyzedFile
	}
	if file == "" {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = "execute command requires a file path (or a prior analyze)"
		return resp
	}

	if isTestFile(file) {
		resp.Status = "error"
		resp.Code = ErrNotSupported
		resp.Message = fmt.Sprintf("_test.go files are not supported targets: %s", file)
		return resp
	}

	if req.Function == nil || *req.Function == "" {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = "execute command requires a function name"
		return resp
	}

	if _, err := os.Stat(file); err != nil {
		resp.Status = "error"
		resp.Code = ErrFileNotFound
		resp.Message = fmt.Sprintf("file not found: %s", file)
		return resp
	}

	// capture defaults to true when omitted (nil), matching protocol semantics.
	capture := req.Capture == nil || *req.Capture

	// --- Adapter dispatch ---
	// If the cached analysis reports an adapter invocation model, resolve the
	// matching hook from the execution profile and dispatch through it instead
	// of the instrumented subprocess harness.
	cacheKey := file + "\x00" + *req.Function
	cachedAnalysis := h.cachedAnalyses[cacheKey]

	// --- Safety policy gate (str-hy9b.G4) ---
	// Classify the target against the default safety policy + any
	// per-target overrides from .shatter/config.yaml. Direct execution
	// paths that touch dangerous side-effect classes are skipped here
	// with an outcome of skipped_by_policy, before any harness is built.
	// Adapter-owned targets (InvocationModel.Kind=="adapter") bypass the
	// gate: they run inside a curated httptest harness whose safety
	// envelope is enforced by the adapter itself.
	if cachedAnalysis != nil && !isAdapterOwned(cachedAnalysis) {
		if decision, applied := h.evaluateExecutePolicy(file, *req.Function, cachedAnalysis); applied && !decision.Allow {
			reason := decision.Reason
			resp.Status = "execute"
			resp.Outcome = &InvocationOutcome{
				Status:      OutcomeStatusSkippedByPolicy,
				ShortReason: &reason,
			}
			return finalizeResponse(resp, timing)
		}
	}

	var runtimeHooks RuntimeHooks
	if len(h.hookFactories) > 0 {
		var projectRoot string
		if req.ProjectRoot != nil {
			projectRoot = *req.ProjectRoot
		}
		var resolveErr error
		runtimeHooks, resolveErr = ResolveRuntimeHooks(req.ExecutionProfile, RuntimeHookContext{
			Phase:        "execute",
			ProjectRoot:  projectRoot,
			EntryFile:    file,
			FunctionName: *req.Function,
		}, h.hookFactories)
		if resolveErr != nil {
			h.log.Debug("adapter resolve failed, falling through to direct", "err", resolveErr)
		}
	}

	var invocationModel *InvocationModel
	if cachedAnalysis != nil {
		invocationModel = cachedAnalysis.InvocationModel
	}
	strategy := ChooseInvocationStrategy(invocationModel, runtimeHooks.InvocationHooks)

	switch strategy.Kind {
	case "unsupported":
		resp.Status = "error"
		resp.Code = ErrNotSupported
		resp.Message = fmt.Sprintf("execution adapter not supported by Go frontend: %s", strategy.AdapterID)
		return resp
	case "adapter":
		finishExecute := timing.Start("execute.total")
		result, err := ExecuteAdapterOwned(strategy.Hook, InvocationContext{
			File:            file,
			FunctionName:    *req.Function,
			InvocationModel: strategy.Model,
			Inputs:          req.Inputs,
			Capture:         capture,
		})
		finishExecute()
		if err != nil {
			resp.Status = "error"
			resp.Code = ErrInternalError
			resp.Message = err.Error()
			return resp
		}
		return mapExecuteResult(resp, result, timing)
	}

	// --- Direct execution via builder/launcher ---

	var execMocks []instrument.MockConfig
	for _, m := range req.Mocks {
		execMocks = append(execMocks, instrument.MockConfig{
			Symbol:           m.Symbol,
			ReturnValues:     m.ReturnValues,
			ShouldTrackCalls: m.ShouldTrackCalls,
			DefaultBehavior:  m.DefaultBehavior,
		})
	}

	var (
		result       *instrument.ExecuteResult
		err          error
		oneShot      *preparedLauncher
		preparedExec preparedExecution
	)

	finishExecute := timing.Start("execute.total")
	if req.PrepareID != nil && *req.PrepareID != "" {
		preparedExec, _ = h.preparedHarnesses[*req.PrepareID]
		if preparedExec != nil && !preparedExec.IsValid() {
			h.log.Warn("prepared harness artifacts missing, rebuilding", "prepare_id", *req.PrepareID)
			preparedExec.Cleanup()
			delete(h.preparedHarnesses, *req.PrepareID)
			preparedExec = nil
		}
		if preparedExec == nil {
			h.log.Debug("stale prepare_id, rebuilding", "prepare_id", *req.PrepareID)
			preparedExec = h.lookupPreparedHarness(file, *req.Function, execMocks)
		}
	} else {
		preparedExec = h.lookupPreparedHarness(file, *req.Function, execMocks)
		if preparedExec != nil {
			h.log.Debug("auto-reusing prepared harness", "file", file, "function", *req.Function)
		}
	}

	if preparedExec == nil {
		oneShot, err = h.prepareDirectExecution(file, *req.Function, execMocks, timing, "execute")
		if err == nil {
			preparedExec = oneShot
		}
	}
	if err == nil {
		finishRun := timing.Start("execute.run")
		result, err = preparedExec.Invoke(req.Inputs, capture)
		finishRun()
	}
	finishExecute()
	if oneShot != nil {
		oneShot.Cleanup()
	}
	if err != nil {
		resp.Status = "error"
		if strings.Contains(err.Error(), "function not found") {
			resp.Code = ErrFunctionNotFound
		} else if strings.Contains(err.Error(), "receiver planning") {
			resp.Code = ErrNotSupported
		} else if strings.Contains(err.Error(), "build failed") {
			resp.Code = ErrInstrumentationFailed
		} else if strings.Contains(err.Error(), "timed out") {
			resp.Code = ErrExecutionTimeout
		} else {
			resp.Code = ErrInternalError
		}
		resp.Message = err.Error()
		resp.Outcome = failureOutcome(err)
		return resp
	}

	return mapExecuteResult(resp, result, timing)
}

// failureOutcome classifies an executor error into an InvocationOutcome. It
// produces the status + short_reason + thrown_error triple required by the
// outcome-driven reporting pipeline (str-hy9b.A2). Callers are responsible
// for retaining legacy error-code + message fields on the Response for
// backwards compatibility with existing consumers.
func failureOutcome(err error) *InvocationOutcome {
	msg := err.Error()
	trimmed := strings.TrimSpace(msg)
	first := trimmed
	if idx := strings.IndexByte(first, '\n'); idx >= 0 {
		first = first[:idx]
	}
	reason := first
	errInfo := &ErrorInfo{ErrorType: "executor_error", Message: msg}
	var status OutcomeStatus
	switch {
	case strings.Contains(msg, "receiver planning"):
		status = OutcomeStatusUnsupported
		reason = "method invocation requires receiver planning (Phase E)"
		errInfo.ErrorType = "method_not_supported"
	case strings.Contains(msg, "function not found"):
		status = OutcomeStatusUnsupported
		reason = "target function not found in source file"
		errInfo.ErrorType = "function_not_found"
	case strings.Contains(msg, "build failed"):
		status = OutcomeStatusBuildFailed
		reason = "go build failed during harness compilation"
		errInfo.ErrorType = "build_failed"
	case strings.Contains(msg, "timed out"):
		status = OutcomeStatusTimedOut
		reason = "execution exceeded the configured timeout"
		errInfo.ErrorType = "execution_timeout"
	default:
		status = OutcomeStatusRuntimeFailed
		errInfo.ErrorType = "runtime_failed"
	}
	return &InvocationOutcome{
		Status:      status,
		ShortReason: &reason,
		ThrownError: errInfo,
	}
}

// mapExecuteResult maps an instrument.ExecuteResult to a protocol Response.
// Shared by both the direct execution path and the adapter-owned path.
func mapExecuteResult(resp Response, result *instrument.ExecuteResult, timing *frontendtiming.Collector) Response {
	resp.Status = "execute"
	resp.ReturnValue = result.ReturnValue
	resp.ThrownError = convertErrorInfo(result.ThrownError)
	resp.LinesExecuted = toIntSlice(result.LinesExecuted)
	resp.BranchPath = convertBranchPath(result.BranchPath)
	resp.PathConstraints = extractPathConstraints(result.BranchPath)
	resp.CallsToExternal = convertExternalCalls(result.ExternalCalls)
	resp.DiscoveredDependencies = convertDiscoveredDeps(result.DiscoveredDependencies)
	resp.SideEffects = convertSideEffects(result.SideEffects)
	resp.ScopeEvents = result.ScopeEvents
	resp.LoopBodyStates = convertLoopBodyStates(result.LoopBodyStates)
	resp.Performance = &PerfMetrics{
		WallTimeMs:         result.Performance.WallTimeMs,
		CPUTimeUs:          result.Performance.CPUTimeUs,
		HeapUsedBytes:      result.Performance.HeapUsedBytes,
		HeapAllocatedBytes: result.Performance.HeapAllocatedBytes,
	}
	resp.Outcome = outcomeFromResult(result, resp.SideEffects, resp.ThrownError)
	return finalizeResponse(resp, timing)
}

// outcomeFromResult synthesizes an InvocationOutcome for a successfully
// executed invocation. A non-nil ThrownError indicates a runtime panic that
// the harness caught and reported without killing the process; we classify
// those as `runtime_failed`, distinguishing them from the host-level build
// and timeout failures that never produce an ExecuteResult at all.
func outcomeFromResult(result *instrument.ExecuteResult, sideEffects []SideEffect, thrownErr *ErrorInfo) *InvocationOutcome {
	if thrownErr != nil {
		kind := thrownErr.ErrorType
		if kind == "" {
			kind = "runtime error"
		}
		status := OutcomeStatusRuntimeFailed
		reason := "invocation raised a " + kind
		if kind == "timeout" {
			status = OutcomeStatusTimedOut
			reason = "execution exceeded the configured timeout"
		}
		return &InvocationOutcome{
			Status:      status,
			ShortReason: &reason,
			ReturnValue: result.ReturnValue,
			ThrownError: thrownErr,
			SideEffects: sideEffects,
		}
	}
	return &InvocationOutcome{
		Status:      OutcomeStatusCompleted,
		ReturnValue: result.ReturnValue,
		SideEffects: sideEffects,
	}
}

func convertLoopBodyStates(states []instrument.LoopBodyState) []LoopBodyState {
	if len(states) == 0 {
		return nil
	}

	result := make([]LoopBodyState, 0, len(states))
	for _, state := range states {
		locals := make(map[string]SymExpr, len(state.Locals))
		for name, rawExpr := range state.Locals {
			var expr SymExpr
			if err := json.Unmarshal(rawExpr, &expr); err == nil {
				locals[name] = expr
			}
		}
		result = append(result, LoopBodyState{
			LoopID:    state.LoopID,
			Iteration: state.Iteration,
			Locals:    locals,
		})
	}
	return result
}

func convertErrorInfo(e *instrument.ErrorInfo) *ErrorInfo {
	if e == nil {
		return nil
	}
	var stack *string
	if e.Stack != "" {
		stack = &e.Stack
	}
	return &ErrorInfo{
		ErrorType: e.ErrorType,
		Message:   e.Message,
		Stack:     stack,
	}
}

func toIntSlice(ints []int) []int {
	if ints == nil {
		return []int{}
	}
	return ints
}

func convertBranchPath(branches []instrument.BranchDecision) []BranchDecision {
	result := make([]BranchDecision, len(branches))
	for i, b := range branches {
		constraint := &SymConstraint{Kind: "unknown", Hint: "no symbolic constraint from Go frontend"}
		if b.ConstraintJSON != "" {
			var sc SymConstraint
			if err := json.Unmarshal([]byte(b.ConstraintJSON), &sc); err == nil {
				constraint = &sc
			}
		}
		result[i] = BranchDecision{
			BranchID:   b.BranchID,
			Line:       b.Line,
			Taken:      b.Taken,
			Constraint: constraint,
		}
	}
	return result
}

func extractPathConstraints(branches []instrument.BranchDecision) []SymConstraint {
	var constraints []SymConstraint
	for _, b := range branches {
		if b.ConstraintJSON == "" {
			continue
		}
		var sc SymConstraint
		if err := json.Unmarshal([]byte(b.ConstraintJSON), &sc); err == nil {
			constraints = append(constraints, sc)
		}
	}
	if constraints == nil {
		return []SymConstraint{}
	}
	return constraints
}

// convertExternalCalls converts executor ExternalCall records to protocol format.
func convertExternalCalls(calls []instrument.ExternalCall) []ExternalCall {
	if len(calls) == 0 {
		return []ExternalCall{}
	}
	result := make([]ExternalCall, len(calls))
	for i, c := range calls {
		args := make([]any, 0)
		if c.Args != nil {
			json.Unmarshal(c.Args, &args) //nolint:errcheck
		}
		var retVal any
		if c.ReturnValue != nil {
			json.Unmarshal(c.ReturnValue, &retVal) //nolint:errcheck
		}
		result[i] = ExternalCall{
			Symbol:      c.Symbol,
			Args:        args,
			ReturnValue: retVal,
		}
	}
	return result
}

// convertDiscoveredDeps converts executor DiscoveredDependency to protocol format.
func convertDiscoveredDeps(deps []instrument.DiscoveredDependency) []DiscoveredDependency {
	if len(deps) == 0 {
		return nil
	}
	result := make([]DiscoveredDependency, len(deps))
	for i, d := range deps {
		result[i] = DiscoveredDependency{
			Symbol:            d.Symbol,
			SourceModule:      d.SourceModule,
			Kind:              d.Kind,
			IsSubprocessSpawn: d.IsSubprocessSpawn,
		}
	}
	return result
}

// convertSideEffects converts executor SideEffects to protocol format.
// All seven canonical kinds are mapped: console_output, file_write,
// network_request, environment_read, global_mutation, thrown_error,
// global_state_change.
func convertSideEffects(effects []instrument.SideEffect) []SideEffect {
	if len(effects) == 0 {
		return []SideEffect{}
	}
	result := make([]SideEffect, len(effects))
	for i, e := range effects {
		result[i] = SideEffect{
			Kind:      e.Kind,
			Level:     e.Level,
			Message:   e.Message,
			Path:      e.Path,
			Content:   e.Content,
			Method:    e.Method,
			URL:       e.URL,
			Body:      e.Body,
			Name:      e.Name,
			ErrorType: e.ErrorType,
			Stack:     e.Stack,
			Value:     e.Value,
			Variable:  e.Variable,
			Before:    e.Before,
			After:     e.After,
		}
	}
	return result
}

// lookupPreparedHarness checks if a prepared harness already exists for the
// given file, function, and mock configuration.
func (h *Handler) lookupPreparedHarness(file, function string, mocks []instrument.MockConfig) preparedExecution {
	prepareID := computePrepareID(file, function, mocks)
	harness, ok := h.preparedHarnesses[prepareID]
	if !ok {
		return nil
	}
	// If the harness's backing artifacts have been deleted externally, prune it.
	if !harness.IsValid() {
		h.log.Warn("pruning prepared harness with missing artifacts", "prepare_id", prepareID)
		harness.Cleanup()
		delete(h.preparedHarnesses, prepareID)
		targetKey := file + "\x00" + function
		if h.preparedTargets[targetKey] == prepareID {
			delete(h.preparedTargets, targetKey)
		}
		return nil
	}
	return harness
}

// pruneOrphans removes harness registrations whose backing source files no
// longer exist on disk. It calls Cleanup() on each orphaned harness to free
// subprocess and artifact resources. Returns the number of entries pruned.
func (h *Handler) pruneOrphans() int {
	pruned := 0
	for targetKey, prepareID := range h.preparedTargets {
		file := strings.SplitN(targetKey, "\x00", 2)[0]
		if _, err := os.Stat(file); err != nil {
			h.log.Debug("pruning orphaned harness registration", "file", file, "prepare_id", prepareID)
			if ph, ok := h.preparedHarnesses[prepareID]; ok {
				ph.Cleanup()
				delete(h.preparedHarnesses, prepareID)
			}
			delete(h.preparedTargets, targetKey)
			pruned++
		}
	}
	return pruned
}

func (h *Handler) handleSetup(resp Response, req Request) Response {
	timing := h.maybeTimingCollector()
	if req.File == "" {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = "setup command requires a file path"
		return resp
	}
	if req.Scope == "" {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = "setup command requires a scope"
		return resp
	}
	if !req.Level.IsValid() {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = fmt.Sprintf("setup command requires a valid level, got %q", req.Level)
		return resp
	}

	if _, err := os.Stat(req.File); err != nil {
		resp.Status = "error"
		resp.Code = ErrFileNotFound
		resp.Message = fmt.Sprintf("setup file not found: %s", req.File)
		return resp
	}

	var parentCtxJSON json.RawMessage
	if req.ParentContext != nil {
		data, err := json.Marshal(req.ParentContext)
		if err != nil {
			resp.Status = "error"
			resp.Code = ErrInternalError
			resp.Message = fmt.Sprintf("marshaling parent context: %v", err)
			return resp
		}
		parentCtxJSON = data
	}

	h.log.Debug("Running setup", "file", req.File, "scope", req.Scope, "level", req.Level)

	finishSetup := timing.Start("setup.total")
	ctx, err := h.setupLoader.RunSetup(req.File, req.Scope, string(req.Level), req.ProjectRoot, parentCtxJSON)
	finishSetup()
	if err != nil {
		resp.Status = "error"
		resp.Code = ErrInternalError
		resp.Message = fmt.Sprintf("setup failed: %v", err)
		return resp
	}

	resp.Status = "setup"
	ctxCopy := json.RawMessage(ctx)
	resp.SetupContext = &ctxCopy
	return finalizeResponse(resp, timing)
}

func (h *Handler) handleTeardown(resp Response, req Request) Response {
	timing := h.maybeTimingCollector()
	if req.Scope == "" {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = "teardown command requires a scope"
		return resp
	}
	if !req.Level.IsValid() {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = fmt.Sprintf("teardown command requires a valid level, got %q", req.Level)
		return resp
	}

	h.log.Debug("Running teardown", "scope", req.Scope, "level", req.Level)

	finishTeardown := timing.Start("teardown.total")
	found := h.setupLoader.Teardown(req.Scope, string(req.Level))
	finishTeardown()
	if !found {
		resp.Status = "error"
		resp.Code = ErrInternalError
		resp.Message = fmt.Sprintf("No setup context found for %s:%s. Call setup first.", req.Level, req.Scope)
		return resp
	}

	// Prune harnesses whose source files have been deleted, then clear remaining
	// harnesses on function-level teardown to free compile artifacts.
	if req.Level == SetupLevelFunction {
		h.pruneOrphans()
		for _, ph := range h.preparedHarnesses {
			ph.Cleanup()
		}
		h.preparedHarnesses = make(map[string]preparedExecution)
		h.preparedTargets = make(map[string]string)
		h.cachedAnalyses = make(map[string]*FunctionAnalysis)
	}

	// Clear stale session state so the next setup/execute cycle starts clean.
	h.lastAnalyzedFile = ""
	h.registry.Handles.Clear()

	resp.Status = "teardown_ack"
	return finalizeResponse(resp, timing)
}

func (h *Handler) handleGenerate(resp Response, req Request) Response {
	timing := h.maybeTimingCollector()
	if req.File == "" {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = "generate command requires a file path"
		return resp
	}
	if req.Name == "" {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = "generate command requires a name"
		return resp
	}

	var recipe json.RawMessage
	if req.Recipe != nil {
		recipe = *req.Recipe
	}

	finishGenerate := timing.Start("generate.total")
	value, generatorID, outRecipe, err := h.registry.Generate(req.File, req.Name, recipe)
	finishGenerate()
	if err != nil {
		resp.Status = "error"
		resp.Code = ErrInternalError
		resp.Message = fmt.Sprintf("generate failed: %v", err)
		return resp
	}

	resp.Status = "generate"
	valCopy := json.RawMessage(value)
	resp.Value = &valCopy
	resp.GeneratorID = generatorID
	if outRecipe != nil {
		recipeCopy := json.RawMessage(outRecipe)
		resp.Recipe = &recipeCopy
	}
	return finalizeResponse(resp, timing)
}

func (h *Handler) handleGetInvocationPlan(resp Response, req Request) Response {
	timing := h.maybeTimingCollector()
	if h.planRequirements == nil {
		resp.Status = "error"
		resp.Code = ErrNotSupported
		resp.Message = "get_invocation_plan: no planner registered in this frontend build"
		return resp
	}

	lookup := func(targetID string) *FunctionAnalysis {
		return h.lookupAnalyzedByTargetID(targetID)
	}

	finishPlan := timing.Start("get_invocation_plan.total")
	plans, unsatisfied := h.planRequirements(req.InvocationRequirements, lookup)
	finishPlan()

	resp.Status = "invocation_plan"
	resp.InvocationPlans = plans
	resp.UnsatisfiedRequirements = unsatisfied
	return finalizeResponse(resp, timing)
}

// lookupAnalyzedByTargetID maps a protocol target_id to a cached
// FunctionAnalysis. The cache is keyed on "file\x00function"; callers of
// get_invocation_plan must have previously issued analyze for the target's
// file so the entry exists. Matching extracts the bare symbol name from
// "pkgPath:QualifiedName" (splitting on the final ":"), then scans cached
// analyses for a matching Name on the most recently analyzed file.
func (h *Handler) lookupAnalyzedByTargetID(targetID string) *FunctionAnalysis {
	bare := bareSymbolFromTargetID(targetID)
	if bare == "" {
		return nil
	}
	if h.lastAnalyzedFile != "" {
		if analysis, ok := h.cachedAnalyses[h.lastAnalyzedFile+"\x00"+bare]; ok {
			return analysis
		}
	}
	// Fall back to linear scan so targets from prior analyses still resolve.
	for key, analysis := range h.cachedAnalyses {
		if analysis.Name != bare {
			continue
		}
		_ = key
		return analysis
	}
	return nil
}

func bareSymbolFromTargetID(targetID string) string {
	idx := strings.LastIndex(targetID, ":")
	if idx < 0 {
		return targetID
	}
	return targetID[idx+1:]
}

// Registry returns the generator registry, allowing custom builds to register
// native generators before calling Run().
func (h *Handler) Registry() *generators.Registry {
	return h.registry
}

// RegisterHookFactory adds a RuntimeHookFactory to the handler. Factories are
// consulted when an ExecutionProfile is present in an execute request.
// Call before Run().
func (h *Handler) RegisterHookFactory(f RuntimeHookFactory) {
	h.hookFactories = append(h.hookFactories, f)
}

func (h *Handler) handleShutdown(resp Response) Response {
	instrument.CloseAllHarnesses()
	h.pruneOrphans()
	for _, ph := range h.preparedHarnesses {
		ph.Cleanup()
	}
	h.preparedHarnesses = make(map[string]preparedExecution)
	h.preparedTargets = make(map[string]string)
	h.cachedAnalyses = make(map[string]*FunctionAnalysis)
	h.registry.Close()
	h.setupLoader.Close()
	resp.Status = "shutdown_ack"
	return resp
}

// parseMajorMinor extracts the major and minor components from a semver string.
// Returns (major, minor, ok). ok is false if the version string is malformed.
func parseMajorMinor(version string) (int, int, bool) {
	parts := strings.SplitN(version, ".", 3)
	if len(parts) < 2 {
		return 0, 0, false
	}
	major, err := strconv.Atoi(parts[0])
	if err != nil {
		return 0, 0, false
	}
	minor, err := strconv.Atoi(parts[1])
	if err != nil {
		return 0, 0, false
	}
	return major, minor, true
}

// isVersionCompatible checks whether a requested protocol version is compatible
// with ProtocolVersion by comparing major and minor components (patch is ignored).
// Matches the TypeScript frontend's semver-compatible behavior.
func isVersionCompatible(version string) bool {
	reqMajor, reqMinor, reqOK := parseMajorMinor(version)
	ourMajor, ourMinor, ourOK := parseMajorMinor(ProtocolVersion)
	if !reqOK || !ourOK {
		return false
	}
	return reqMajor == ourMajor && reqMinor == ourMinor
}

func (h *Handler) send(resp Response) error {
	data, err := json.Marshal(resp)
	if err != nil {
		return fmt.Errorf("marshaling response: %w", err)
	}
	line := string(data) + "\n"
	h.log.Log(context.Background(), LevelTrace, "Sent", "raw", string(data))
	_, err = io.WriteString(h.writer, line)
	return err
}
