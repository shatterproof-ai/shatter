package protocol

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"os"
	"strconv"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/generators"
	"github.com/shatter-dev/shatter/shatter-go/instrument"
	"github.com/shatter-dev/shatter/shatter-go/setup"
	frontendtiming "github.com/shatter-dev/shatter/shatter-go/timing"
)

const frontendVersion = "0.1.0"
const frontendLanguage = "go"

// Handler processes protocol requests and writes responses.
type Handler struct {
	reader           *bufio.Scanner
	writer           io.Writer
	log              *slog.Logger
	lastAnalyzedFile string // remembered from the most recent analyze command
	registry         *generators.Registry
	setupLoader      *setup.Loader
	timingEnabled    bool
}

// NewHandler creates a handler reading from r, writing responses to w,
// and logging to logw at the level set by SHATTER_LOG_LEVEL.
func NewHandler(r io.Reader, w io.Writer, logw io.Writer) *Handler {
	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 0, 1024*1024), 10*1024*1024) // 10MB max line
	return &Handler{
		reader:      scanner,
		writer:      w,
		log:         slog.New(newPrefixHandler(logw, slogLevelFromEnv())),
		registry:    generators.NewRegistry(),
		setupLoader: setup.NewLoader(),
	}
}

// NewHandlerWithLogLevel creates a handler with an explicit log level (for testing).
func NewHandlerWithLogLevel(r io.Reader, w io.Writer, logw io.Writer, level string) *Handler {
	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 0, 1024*1024), 10*1024*1024)
	return &Handler{
		reader:      scanner,
		writer:      w,
		log:         slog.New(newPrefixHandler(logw, slogLevelFromString(level))),
		registry:    generators.NewRegistry(),
		setupLoader: setup.NewLoader(),
	}
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
	case "execute":
		return h.handleExecute(base, req), false
	case "setup":
		return h.handleSetup(base, req), false
	case "teardown":
		return h.handleTeardown(base, req), false
	case "generate":
		return h.handleGenerate(base, req), false
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

func (h *Handler) handleAnalyze(resp Response, req Request) Response {
	timing := h.maybeTimingCollector()
	if req.File == "" {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = "analyze command requires a file path"
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
	functions, err := AnalyzeFileWithTiming(req.File, functionName, timing)
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
	resp.Functions = functions
	return finalizeResponse(resp, timing)
}

func isNotFound(err error) bool {
	return err != nil && strings.HasPrefix(err.Error(), "function not found")
}

func (h *Handler) handleInstrument(resp Response, req Request) Response {
	timing := h.maybeTimingCollector()
	if req.File == "" {
		resp.Status = "error"
		resp.Code = ErrInvalidRequest
		resp.Message = "instrument command requires a file path"
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

	// Convert protocol MockConfigs to instrument MockConfigs and pass to executor.
	var execMocks []instrument.MockConfig
	for _, m := range req.Mocks {
		execMocks = append(execMocks, instrument.MockConfig{
			Symbol:           m.Symbol,
			ReturnValues:     m.ReturnValues,
			ShouldTrackCalls: m.ShouldTrackCalls,
			DefaultBehavior:  m.DefaultBehavior,
		})
	}
	// capture defaults to true when omitted (nil), matching protocol semantics.
	capture := req.Capture == nil || *req.Capture
	finishExecute := timing.Start("execute.total")
	result, err := instrument.ExecuteFunctionWithTiming(file, *req.Function, req.Inputs, timing, capture, execMocks)
	finishExecute()
	if err != nil {
		resp.Status = "error"
		if strings.Contains(err.Error(), "function not found") {
			resp.Code = ErrFunctionNotFound
		} else if strings.Contains(err.Error(), "build failed") {
			resp.Code = ErrInstrumentationFailed
		} else if strings.Contains(err.Error(), "timed out") {
			resp.Code = ErrExecutionTimeout
		} else {
			resp.Code = ErrInternalError
		}
		resp.Message = err.Error()
		return resp
	}

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
	resp.Performance = &PerfMetrics{
		WallTimeMs:         result.Performance.WallTimeMs,
		CPUTimeUs:          result.Performance.CPUTimeUs,
		HeapUsedBytes:      result.Performance.HeapUsedBytes,
		HeapAllocatedBytes: result.Performance.HeapAllocatedBytes,
	}

	return finalizeResponse(resp, timing)
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
		var args []any
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

// Registry returns the generator registry, allowing custom builds to register
// native generators before calling Run().
func (h *Handler) Registry() *generators.Registry {
	return h.registry
}

func (h *Handler) handleShutdown(resp Response) Response {
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
