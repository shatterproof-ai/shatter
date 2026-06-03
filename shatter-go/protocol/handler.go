package protocol

import (
	"bufio"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"go/ast"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"runtime/debug"
	"sort"
	"strconv"
	"strings"

	"golang.org/x/tools/go/packages"

	"github.com/shatter-dev/shatter/shatter-go/config"
	"github.com/shatter-dev/shatter/shatter-go/generators"
	"github.com/shatter-dev/shatter/shatter-go/instrument"
	goloader "github.com/shatter-dev/shatter/shatter-go/loader"
	"github.com/shatter-dev/shatter/shatter-go/setup"
	frontendtiming "github.com/shatter-dev/shatter/shatter-go/timing"
	"github.com/shatter-dev/shatter/shatter-go/workspace"
	"github.com/shatter-dev/shatter/shatter-go/wrapper"
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
	preparedTargets   map[string]string // "file\x00function\x00receiverKind" → current prepare_id for stale detection (str-oegu)
	hookFactories     []RuntimeHookFactory
	cachedAnalyses    map[string]*FunctionAnalysis // "file\x00function" → cached analysis

	// Environment preflight state (str-1hlk.18.1). Once any project_root
	// fails the missing-go.mod check, the failure is sticky and every
	// subsequent analyze/instrument/prepare/execute/setup short-circuits
	// with a single preflight_failed error instead of N noisy compilation
	// or runtime errors. Mirrors shatter-ts/src/handlers.ts:148-230.
	preflightFail         *preflightFailure
	preflightCheckedRoots map[string]struct{}
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
		reader:                scanner,
		writer:                w,
		log:                   slog.New(newPrefixHandler(logw, level)),
		workspace:             workspace,
		registry:              generators.NewRegistry(),
		setupLoader:           setup.NewLoader(),
		preparedHarnesses:     make(map[string]preparedExecution),
		preparedTargets:       make(map[string]string),
		cachedAnalyses:        make(map[string]*FunctionAnalysis),
		preflightCheckedRoots: make(map[string]struct{}),
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
// its TargetContext (analysis + DiscoveredTarget + same-package constructors)
// or nil when the target was not analyzed.
//
// The handler builds TargetContext on demand for each requirement: cached
// FunctionAnalysis lookup as today, plus on-demand package reload to recover
// Receiver shape and HasTypeParams (Go-internal fields that are not on the
// wire FunctionAnalysis) and to scan same-package constructor candidates.
// See handler.handleGetInvocationPlan and handler.buildTargetContext.
type PlannerFunc func(
	requirements []InvocationRequirement,
	lookup func(targetID string) *TargetContext,
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
			// Best-effort: recover the request id from the raw line even
			// when the surrounding JSON is malformed so the error response
			// still aligns with the pending request on the core side
			// (str-jeen.52). A request whose id is unrecoverable falls back
			// to 0; pairing that with a non-zero pending request would surface
			// as IdMismatch on the core, which is the desired loud-fail.
			recoveredID := bestEffortRequestID(line)
			errResp := Response{
				ProtocolVersion: ProtocolVersion,
				ID:              recoveredID,
				Status:          "error",
				Code:            ErrInvalidRequest,
				Message:         fmt.Sprintf("Invalid JSON: %s", err.Error()),
			}
			if sendErr := h.send(errResp); sendErr != nil {
				return fmt.Errorf("writing error response: %w", sendErr)
			}
			continue
		}

		resp, shutdown := h.safeDispatch(req)
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

// safeDispatch wraps dispatch with a panic recovery that emits a properly
// id-tagged error response for the in-flight request (str-jeen.52). Without
// this guard a panic in any handle* path would terminate the frontend
// without writing a response for `req`, leaving a pending request on the
// core side; once a respawned frontend served the next request, the core's
// stdout reader could surface a stale buffered response from the dead
// process and report a `response id N does not match request id N+1`
// IdMismatch instead of the underlying crash. Recovering keeps the
// request/response pairing intact: every request id is answered exactly
// once on the same protocol stream, with `internal_error` carrying the
// recovered value and stack so the failure surfaces loudly without
// corrupting downstream id alignment.
func (h *Handler) safeDispatch(req Request) (resp Response, shutdown bool) {
	defer func() {
		if r := recover(); r != nil {
			h.log.Error("panic during dispatch",
				"id", req.ID, "command", req.Command, "panic", r)
			resp = Response{
				ProtocolVersion: ProtocolVersion,
				ID:              req.ID,
				Status:          "error",
				Code:            ErrInternalError,
				Message: fmt.Sprintf(
					"panic during %s: %v\n%s",
					req.Command, r, debug.Stack(),
				),
			}
			shutdown = false
		}
	}()
	return h.dispatch(req)
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
		"complex_type:go_byte",
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

// Environment preflight (str-1hlk.18.1).
//
// When a directory isn't a Go module (no `go.mod`), every per-target
// execute fails during `go build` with `cannot find main module` and the
// run report fills with N noisy compilation_error rows whose root cause
// is a single env-setup miss. To surface the env failure
// once and suppress the per-target noise, the frontend runs a one-shot
// preflight on the first request that carries a `project_root`. If the
// check fails, the failure is cached and every subsequent
// analyze/instrument/prepare/execute/setup short-circuits with the same
// error response. Mirrors shatter-ts/src/handlers.ts:148-230 (str-jeen.40).
const (
	preflightReasonMissingGoMod = "missing_go_mod"
	preflightGoModFile          = "go.mod"
)

type preflightFailure struct {
	reason string
	detail string
}

// runPreflight is idempotent per project_root. Once any root fails the
// failure is sticky — a later root with go.mod does not clear it, because
// in a single run multiple targets share the same env and one failure is
// authoritative.
func (h *Handler) runPreflight(projectRoot *string) {
	if h.preflightFail != nil || projectRoot == nil || *projectRoot == "" {
		return
	}
	root := *projectRoot
	if _, seen := h.preflightCheckedRoots[root]; seen {
		return
	}
	h.preflightCheckedRoots[root] = struct{}{}
	goModPath := filepath.Join(root, preflightGoModFile)
	if _, err := os.Stat(goModPath); err != nil {
		h.preflightFail = &preflightFailure{
			reason: preflightReasonMissingGoMod,
			detail: goModPath,
		}
	}
}

func (h *Handler) preflightErrorResponse(resp Response) Response {
	f := h.preflightFail
	resp.Status = "error"
	resp.Code = ErrPreflightFailed
	resp.Message = fmt.Sprintf("preflight_failed: %s: %s", f.reason, f.detail)
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

	// file_not_found takes priority over env preflight — a typo'd path is
	// more actionable, and TS/Rust agree on this ordering
	// (shatter-ts/src/handlers.ts:382-400).
	h.runPreflight(req.ProjectRoot)
	if h.preflightFail != nil {
		return h.preflightErrorResponse(resp)
	}

	if isGeneratedFile(req.File) {
		resp.Status = "error"
		resp.Code = ErrNotSupported
		resp.Message = fmt.Sprintf("generated files are skipped by default: %s", req.File)
		return resp
	}

	h.lastAnalyzedFile = req.File

	var functionName string
	if req.Function != nil {
		functionName = *req.Function
	}

	// Discovery cache (str-hy9b.C6): consult <workspace>/analysis/<hash>.json
	// before running the analyzer. Hash inputs cover the target package's
	// source files, one-level imports, the Go runtime version, and the
	// Shatter protocol version; any mismatch produces a miss and the
	// analyzer recomputes. The cache is best-effort — hash errors and write
	// errors are logged but never block the analysis path.
	var (
		cacheHash    string
		cacheHashErr error
	)
	if h.workspace != nil {
		cacheHash, cacheHashErr = ComputeDiscoveryHash(req.File, functionName)
		if cacheHashErr == nil {
			if cached, hit, missReason := ReadAnalysisCache(h.workspace, cacheHash); hit {
				// Initialize the loader even on a cache hit so
				// handleGetInvocationPlan can rebuild method DiscoveredTargets
				// (Receiver shape / HasTypeParams) by reloading the package.
				// The loader itself caches packages, so this is cheap on
				// repeated lookups.
				if err := h.ensureLoader(); err != nil {
					h.log.Debug("analysis cache hit: ensureLoader failed; falling through to full analyze",
						"file", req.File, "err", err)
				} else {
					logCacheHit(h.log, cacheHash, req.File)
					return h.finalizeAnalyzeFromCache(resp, req, cached, timing)
				}
			} else {
				logCacheMiss(h.log, cacheHash, req.File, missReason)
			}
		} else {
			h.log.Debug("analysis cache hash failed", "file", req.File, "err", cacheHashErr)
		}
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
		// Build-tag-excluded files surface as NotSupported so the Rust core's
		// batch_analyze soft-skip path consumes them rather than aborting on
		// a ParseError. See str-8amu.
		var buildTagErr *BuildTagExcludedError
		if errors.As(err, &buildTagErr) {
			resp.Status = "error"
			resp.Code = ErrNotSupported
			resp.Message = buildTagErr.Error()
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
	// decide whether to dispatch through an adapter-owned hook. Populating
	// SourceFile here gives the planner closure (str-hy9b.G3) the file
	// context it needs to resolve hint_config_v1 entries per target without
	// changing the PlannerFunc signature.
	for i := range functions {
		if functions[i].SourceFile == "" {
			functions[i].SourceFile = req.File
		}
		key := req.File + "\x00" + functions[i].Name
		h.cachedAnalyses[key] = &functions[i]
	}

	// Write the discovery cache only on full-analyze paths where hashing
	// succeeded; a write failure is logged at warn level but does not affect
	// the response (the next run will simply recompute).
	if h.workspace != nil && cacheHashErr == nil && cacheHash != "" {
		if writeErr := WriteAnalysisCache(h.workspace, cacheHash, req.File, functionName, functions); writeErr != nil {
			h.log.Warn("analysis cache write failed", "hash", cacheHash, "file", req.File, "err", writeErr)
		} else {
			logCacheWrite(h.log, cacheHash, req.File)
		}
	}

	resp.Functions = functions
	return finalizeResponse(resp, timing)
}

// finalizeAnalyzeFromCache populates the in-memory cachedAnalyses map and the
// response Functions slice from a cache hit, mirroring the post-analyze
// bookkeeping in handleAnalyze. Kept separate so the cache-hit path skips
// timing.Start("analyze.total") and the analyzer call, while still
// publishing every FunctionAnalysis to the execute-side cache.
func (h *Handler) finalizeAnalyzeFromCache(resp Response, req Request, cached []FunctionAnalysis, timing *frontendtiming.Collector) Response {
	resp.Status = "analyze"
	if cached == nil {
		cached = []FunctionAnalysis{}
	}
	for i := range cached {
		if cached[i].SourceFile == "" {
			cached[i].SourceFile = req.File
		}
		key := req.File + "\x00" + cached[i].Name
		h.cachedAnalyses[key] = &cached[i]
	}
	resp.Functions = cached
	return finalizeResponse(resp, timing)
}

func (h *Handler) cachedAnalysisForPolicy(file, function string, timing *frontendtiming.Collector) *FunctionAnalysis {
	cacheKey := file + "\x00" + function
	if cached := h.cachedAnalyses[cacheKey]; cached != nil {
		return cached
	}

	finishAnalyze := timing.Start("policy.analyze")
	functions, err := h.analyzeFile(file, "", timing)
	finishAnalyze()
	if err != nil {
		h.log.Debug("policy analysis failed; proceeding without policy cache", "file", file, "function", function, "err", err)
		return nil
	}

	var found *FunctionAnalysis
	for i := range functions {
		if functions[i].SourceFile == "" {
			functions[i].SourceFile = file
		}
		key := file + "\x00" + functions[i].Name
		h.cachedAnalyses[key] = &functions[i]
		if functions[i].Name == function {
			found = &functions[i]
		}
	}
	return found
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
	if err := h.ensureLoader(); err != nil {
		return nil, err
	}
	return AnalyzeFileWithLoaderAndTiming(filePath, functionName, h.loader, timing)
}

// ensureLoader lazy-initializes h.loader from h.workspace. Both the
// analyze-cache miss path (analyzeFile) and the analyze-cache hit path need
// the loader available because handleGetInvocationPlan's TargetContext
// builder (buildTargetContext) reloads the package through h.loader to
// recover Receiver / HasTypeParams off the wire (str-hy9b.C6: cache hit
// must not regress receiver-aware planning).
func (h *Handler) ensureLoader() error {
	if h.workspace == nil || h.loader != nil {
		return nil
	}
	ldr, err := goloader.New(h.workspace)
	if err != nil {
		return fmt.Errorf("construct analyzer loader: %w", err)
	}
	h.loader = ldr
	return nil
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

	h.runPreflight(req.ProjectRoot)
	if h.preflightFail != nil {
		return h.preflightErrorResponse(resp)
	}

	if isGeneratedFile(req.File) {
		resp.Status = "error"
		resp.Code = ErrNotSupported
		resp.Message = fmt.Sprintf("generated files are skipped by default: %s", req.File)
		return resp
	}

	h.lastAnalyzedFile = req.File

	finishInstrument := timing.Start("instrument.total")
	ws, err := h.ensureWorkspace(req.File)
	if err != nil {
		finishInstrument()
		resp.Status = "error"
		resp.Code = ErrInternalError
		resp.Message = fmt.Sprintf("initialize workspace: %v", err)
		return resp
	}
	outputDir, err := os.MkdirTemp(ws.GeneratedDir(), "instrument-*")
	if err == nil {
		err = instrument.MaterializeInstrumentedDirectory(req.File, req.Function, outputDir, req.ProjectRoot, timing)
	}
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
// file path, function name, sorted mock symbols, receiver_kind, and
// generic_type_args. Two callers with different plan dispatch values produce
// different IDs so plan-aware callers can pre-build the right wrapper case.
func computePrepareID(file, function string, mocks []instrument.MockConfig, receiverKind string, genericTypeArgs ...string) string {
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
	fmt.Fprintf(h, "%s\x00", receiverKind)
	for _, arg := range genericTypeArgs {
		fmt.Fprintf(h, "%s\x00", arg)
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
	h.runPreflight(req.ProjectRoot)
	if h.preflightFail != nil {
		return h.preflightErrorResponse(resp)
	}
	if _, err := os.Stat(file); err != nil {
		resp.Status = "error"
		resp.Code = ErrFileNotFound
		resp.Message = fmt.Sprintf("file not found: %s", file)
		return resp
	}
	if isGeneratedFile(file) {
		resp.Status = "error"
		resp.Code = ErrNotSupported
		resp.Message = fmt.Sprintf("generated files are skipped by default: %s", file)
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

	// Extract receiver_kind from the plan when present so the prepare_id
	// keys on (file, function, mocks, receiver_kind), allowing plan-aware
	// callers to pre-build the right wrapper case (str-oegu).
	receiverKind := ""
	var genericTypeArgs []string
	if req.Plan != nil {
		receiverKind = req.Plan.ReceiverKind
		genericTypeArgs = append([]string{}, req.Plan.GenericTypeArgs...)
	}

	prepareID := computePrepareID(file, *req.Function, execMocks, receiverKind, genericTypeArgs...)
	targetKey := file + "\x00" + *req.Function + "\x00" + receiverKind + "\x00" + strings.Join(genericTypeArgs, "\x00")

	if cachedAnalysis := h.cachedAnalysisForPolicy(file, *req.Function, timing); cachedAnalysis != nil && !isAdapterOwned(cachedAnalysis) {
		if decision, applied := h.evaluateExecutePolicy(file, *req.Function, cachedAnalysis); applied && !decision.Allow {
			h.log.Debug("skipping prepare by execution policy", "file", file, "function", *req.Function, "reason", decision.Reason)
			resp.Status = "prepare"
			resp.PrepareID = prepareID
			return finalizeResponse(resp, timing)
		}
	}

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

	h.prunePreparedHarnessesBeforeNewPrepare(prepareID)
	h.log.Debug("Preparing harness", "file", file, "function", *req.Function, "prepare_id", prepareID)

	finishPrepare := timing.Start("prepare.total")
	harness, err := h.prepareDirectExecution(file, *req.Function, execMocks, timing, "prepare", receiverKind, genericTypeArgs)
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

	h.runPreflight(req.ProjectRoot)
	if h.preflightFail != nil {
		return h.preflightErrorResponse(resp)
	}

	if _, err := os.Stat(file); err != nil {
		resp.Status = "error"
		resp.Code = ErrFileNotFound
		resp.Message = fmt.Sprintf("file not found: %s", file)
		return resp
	}

	if isGeneratedFile(file) {
		resp.Status = "error"
		resp.Code = ErrNotSupported
		resp.Message = fmt.Sprintf("generated files are skipped by default: %s", file)
		return resp
	}

	// capture defaults to true when omitted (nil), matching protocol semantics.
	capture := req.Capture == nil || *req.Capture

	// --- Adapter dispatch ---
	// If the cached analysis reports an adapter invocation model, resolve the
	// matching hook from the execution profile and dispatch through it instead
	// of the instrumented subprocess harness.
	cachedAnalysis := h.cachedAnalysisForPolicy(file, *req.Function, timing)
	if shouldForceDirectReceiverExecution(*req.Function, cachedAnalysis) {
		receiverAnalysis := *cachedAnalysis
		receiverAnalysis.InvocationModel = nil
		cachedAnalysis = &receiverAnalysis
	}

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
			resp.Performance = &PerfMetrics{}
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

	// When the request carries a non-nil Plan (str-hy9b.H5), thread the
	// plan's receiver_kind into Invoke so the wrapper's switch dispatches
	// against the right constructor / zero-value strategy. Plan-less
	// requests keep the legacy free-function path. The plan's target_id
	// is intentionally NOT honored here — the prepared launcher knows its
	// own target_id (the wrapper's source of truth), and a mismatched
	// caller-provided id would only confuse the launcher's case lookup.
	// Extract early so lookupPreparedHarness can key on receiver_kind (str-oegu).
	requestReceiverKind := ""
	var requestGenericTypeArgs []string
	if req.Plan != nil {
		requestReceiverKind = req.Plan.ReceiverKind
		requestGenericTypeArgs = append([]string{}, req.Plan.GenericTypeArgs...)
	}

	// Synthesize a default receiver_kind for method targets that arrive
	// without a usable plan (str-jeen.50). Pre-fix, these requests reached
	// the wrapper's default arm with an empty receiver_kind, surfaced as a
	// misleading `runtime_failed` "unknown receiver kind" error, and were
	// then counted as completed exploration by the broad-scan reporter.
	// Now: methods with a constructible receiver get a valid receiver_kind
	// threaded through; methods with no constructible receiver short-circuit
	// to an `unsupported` outcome before the launcher runs.
	if requestReceiverKind == "" {
		synthKind, unsat := h.synthesizeExecuteReceiverKind(file, *req.Function)
		if unsat != nil {
			reason := receiverUnsupportedReason(unsat)
			resp.Status = "execute"
			resp.Outcome = &InvocationOutcome{
				Status:      OutcomeStatusUnsupported,
				ShortReason: &reason,
				ThrownError: &ErrorInfo{
					ErrorType: "method_not_supported",
					Message:   reason,
				},
			}
			resp.Performance = &PerfMetrics{}
			return finalizeResponse(resp, timing)
		}
		requestReceiverKind = synthKind
	}

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
			preparedExec = h.lookupPreparedHarness(file, *req.Function, execMocks, requestReceiverKind, requestGenericTypeArgs...)
		}
	} else {
		preparedExec = h.lookupPreparedHarness(file, *req.Function, execMocks, requestReceiverKind, requestGenericTypeArgs...)
		if preparedExec != nil {
			h.log.Debug("auto-reusing prepared harness", "file", file, "function", *req.Function)
		}
	}

	if preparedExec == nil && err == nil {
		oneShot, err = h.prepareDirectExecution(file, *req.Function, execMocks, timing, "execute", requestReceiverKind, requestGenericTypeArgs)
		if err == nil {
			preparedExec = oneShot
		}
	}
	if err == nil {
		finishRun := timing.Start("execute.run")
		if planAware, ok := preparedExec.(interface {
			InvokeWithPlan(string, []string, []json.RawMessage, bool) (*instrument.ExecuteResult, error)
		}); ok {
			result, err = planAware.InvokeWithPlan(requestReceiverKind, requestGenericTypeArgs, req.Inputs, capture)
		} else {
			result, err = preparedExec.InvokeWithReceiverKind(requestReceiverKind, req.Inputs, capture)
		}
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
		resp.Performance = &PerfMetrics{}
		return resp
	}

	if cachedAnalysis != nil && len(result.LoopBodyStates) == 0 {
		result.LoopBodyStates = buildLoopBodyStatesFromAnalysis(cachedAnalysis, result.ScopeEvents)
	}
	return mapExecuteResult(resp, result, timing)
}

func shouldForceDirectReceiverExecution(functionName string, analysis *FunctionAnalysis) bool {
	if analysis == nil || analysis.InvocationModel == nil {
		return false
	}
	if analysis.InvocationModel.Kind != "adapter" {
		return false
	}
	return isReceiverQualifiedFunctionName(functionName)
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
	case strings.Contains(msg, "unknown receiver kind"):
		// Defense-in-depth (str-jeen.50): the wrapper's default arm
		// emits this when a method target reaches it with an empty or
		// unrecognised receiver_kind. handleExecute now synthesizes a
		// default receiver_kind before invocation, so this arm should
		// rarely fire — but if a caller bypasses synthesis (e.g. by
		// hand-crafting a Plan with a bogus receiver_kind), classify
		// the failure as `unsupported` rather than `runtime_failed`.
		// The pre-str-jeen.50 default of runtime_failed caused these
		// wrapper-level structural errors to be counted as successful
		// completed exploration outcomes.
		status = OutcomeStatusUnsupported
		reason = "method invocation reached wrapper with no constructible receiver"
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
	case strings.Contains(msg, "subprocess exited"):
		// str-jeen.80: launcher subprocess died before producing a
		// response. The error message carries the binary path, exit
		// status, and captured stderr — preserve it as the reason so
		// the CLI report shows the underlying command and output
		// rather than an opaque runtime failure.
		status = OutcomeStatusRuntimeFailed
		reason = "launcher subprocess exited before responding"
		errInfo.ErrorType = "subprocess_crashed"
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

func buildLoopBodyStatesFromScopeEvents(loops []LoopInfo, scopeEvents []json.RawMessage) []instrument.LoopBodyState {
	if len(loops) == 0 || len(scopeEvents) == 0 {
		return nil
	}

	knownLoopIDs := make(map[int]bool, len(loops))
	for _, loop := range loops {
		knownLoopIDs[loop.LoopID] = true
	}

	type scopeEvent struct {
		Kind   string `json:"kind"`
		LoopID *int   `json:"loop_id,omitempty"`
	}
	type traceEvent struct {
		Type  string      `json:"type"`
		Kind  string      `json:"kind"`
		Event *scopeEvent `json:"event,omitempty"`
	}

	iterations := make(map[int]int, len(loops))
	states := make([]instrument.LoopBodyState, 0)
	for _, raw := range scopeEvents {
		var event traceEvent
		if err := json.Unmarshal(raw, &event); err != nil {
			continue
		}
		if event.Type != "scope" && event.Kind != "scope" {
			continue
		}
		if event.Event == nil || event.Event.Kind != "loop_enter" || event.Event.LoopID == nil {
			continue
		}
		loopID := *event.Event.LoopID
		if !knownLoopIDs[loopID] {
			continue
		}
		iteration := iterations[loopID]
		iterations[loopID] = iteration + 1
		states = append(states, instrument.LoopBodyState{
			LoopID:    loopID,
			Iteration: iteration,
			Locals:    map[string]json.RawMessage{},
		})
	}
	return states
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
// given file, function, mock configuration, and receiver kind (str-oegu).
func (h *Handler) lookupPreparedHarness(file, function string, mocks []instrument.MockConfig, receiverKind string, genericTypeArgs ...string) preparedExecution {
	prepareID := computePrepareID(file, function, mocks, receiverKind, genericTypeArgs...)
	harness, ok := h.preparedHarnesses[prepareID]
	if !ok {
		return nil
	}
	// If the harness's backing artifacts have been deleted externally, prune it.
	if !harness.IsValid() {
		h.log.Warn("pruning prepared harness with missing artifacts", "prepare_id", prepareID)
		harness.Cleanup()
		delete(h.preparedHarnesses, prepareID)
		targetKey := file + "\x00" + function + "\x00" + receiverKind + "\x00" + strings.Join(genericTypeArgs, "\x00")
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

func (h *Handler) prunePreparedHarnessesBeforeNewPrepare(keepID string) {
	for prepareID, harness := range h.preparedHarnesses {
		if prepareID == keepID {
			continue
		}
		harness.Cleanup()
		delete(h.preparedHarnesses, prepareID)
	}
	for targetKey, prepareID := range h.preparedTargets {
		if prepareID != keepID {
			delete(h.preparedTargets, targetKey)
		}
	}
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

	h.runPreflight(req.ProjectRoot)
	if h.preflightFail != nil {
		return h.preflightErrorResponse(resp)
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

	lookup := func(targetID string) *TargetContext {
		return h.buildTargetContext(targetID)
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
	analysis, _ := h.lookupAnalyzedLocation(targetID)
	return analysis
}

// lookupAnalyzedLocation is like lookupAnalyzedByTargetID but additionally
// returns the file path the analysis came from. Callers that need to reload
// the parsed package (e.g. to recover Receiver shape or to scan constructors)
// use the file path as input to loadPackageForAnalysis.
//
// Returns (nil, "") when the target is not in the analysis cache.
func (h *Handler) lookupAnalyzedLocation(targetID string) (*FunctionAnalysis, string) {
	bare := bareSymbolFromTargetID(targetID)
	if bare == "" {
		return nil, ""
	}
	if h.lastAnalyzedFile != "" {
		if analysis, ok := h.cachedAnalyses[h.lastAnalyzedFile+"\x00"+bare]; ok {
			return analysis, h.lastAnalyzedFile
		}
	}
	// Fall back to linear scan so targets from prior analyses still resolve.
	for key, analysis := range h.cachedAnalyses {
		if analysis.Name != bare {
			continue
		}
		// key is "file\x00function"; recover the file prefix.
		if idx := strings.IndexByte(key, '\x00'); idx >= 0 {
			return analysis, key[:idx]
		}
		return analysis, ""
	}
	return nil, ""
}

// buildTargetContext is the planner's TargetLookup-shaped closure. It
// resolves a target_id into a TargetContext suitable for both the free-
// function and the receiver-aware planner paths.
//
// The handler rebuilds the Go-internal DiscoveredTarget from the parsed
// package for both free functions and methods so the planner can read generic
// type parameters, receiver shape, and HasTypeParams. For method targets it
// also scans same-package constructor candidates whose TargetType matches the
// receiver type. FunctionAnalysis is the wire shape, and it does not expose
// enough target metadata for receiver or generic planning on its own.
//
// On any error (cache miss, package load failure, FuncDecl not found in pkg)
// the returned TargetContext omits Target and Constructors; the planner then
// follows its free-function path or surfaces NoConstructor depending on what
// it sees. Callers can distinguish "no analyze cache" (returns nil) from
// "method without resolvable receiver" (Target nil but Analysis set).
func (h *Handler) buildTargetContext(targetID string) *TargetContext {
	analysis, file := h.lookupAnalyzedLocation(targetID)
	if analysis == nil {
		return nil
	}
	ctx := &TargetContext{Analysis: analysis}

	// Always load the package when possible: the analyzer emits a bare
	// function name (`fn.Name.Name`) whether the symbol is a free function
	// or a method, so we cannot tell the two apart from FunctionAnalysis
	// alone. The loader caches packages, so repeat lookups within a
	// session are cheap.
	if h.loader == nil || file == "" {
		return ctx
	}

	pkg, err := loadPackageForAnalysis(h.loader, file)
	if err != nil || pkg == nil || pkg.Fset == nil {
		return ctx
	}

	fn := findFuncDeclByBareName(pkg, analysis.Name)
	if fn == nil {
		return ctx
	}

	target := BuildDiscoveredTarget(pkg.Fset, fn, pkg.TypesInfo, pkg.PkgPath, pkg.Name, file)
	ctx.Target = &target

	if target.Receiver != nil && target.Receiver.TypeName != "" {
		all := ScanConstructors(pkg)
		recvType := target.Receiver.TypeName
		var matched []ConstructorCandidate
		for _, c := range all {
			if c.TargetType == recvType {
				matched = append(matched, c)
			}
		}
		ctx.Constructors = matched
		ctx.ReceiverRequiresConstruction = ReceiverRequiresConstruction(pkg, &target)
	}

	// str-4v9h: discover interface implementation candidates for parameters
	// typed as imported interfaces. The defining package must already be
	// loaded in pkg.Imports (satisfied when the consumer imports the type).
	ctx.InterfaceImplsByParam = discoverInterfaceImplCandidates(pkg, fn)
	ctx.JSONEncodeInterfaceParams = discoverJSONEncodeInterfaceParams(pkg.TypesInfo, fn)

	return ctx
}

// synthesizeExecuteReceiverKind selects a default receiver_kind for an
// Execute request that names a method target but did not carry an
// InvocationPlan (str-jeen.50). It returns:
//
//   - ("", nil) when the target is a free function — the caller should keep
//     the empty receiver_kind and let the wrapper's free-function arm fire.
//   - (kind, nil) when synthesis succeeded; kind is a wrapper-facing token
//     ("zero_value" or "constructor:<FuncName>") that matches one of the
//     cases the wrapper emits for this target.
//   - ("", unsat) when the receiver has no constructible strategy (interface
//     receiver, generic-unconstrained, etc.). The caller should short-circuit
//     with an `unsupported` outcome instead of invoking the launcher; without
//     this guard the wrapper's default arm would emit "unknown receiver kind"
//     and the failure would be misclassified as a completed exploration.
//
// On any cache miss or package-load failure synthesize returns ("", nil) so
// the caller proceeds with the legacy empty-receiver path; that path now
// surfaces a clean `unsupported` outcome via failureOutcome's
// "unknown receiver kind" arm rather than a misclassified `runtime_failed`.
func (h *Handler) synthesizeExecuteReceiverKind(file string, function string) (string, *UnsatisfiedRequirement) {
	if file == "" || function == "" {
		return "", nil
	}
	if _, _, err := h.ensureExecutionLoader(file); err != nil || h.loader == nil {
		return "", nil
	}
	pkg, err := loadPackageForAnalysis(h.loader, file)
	if err != nil || pkg == nil || pkg.Fset == nil {
		return "", nil
	}
	fn := findFuncDeclByBareName(pkg, function)
	if fn == nil {
		return "", nil
	}
	target := BuildDiscoveredTarget(pkg.Fset, fn, pkg.TypesInfo, pkg.PkgPath, pkg.Name, file)
	if target.Receiver == nil {
		return "", nil
	}
	if target.Receiver.IsInterface {
		return "", &UnsatisfiedRequirement{
			Kind:     UnsatisfiedRequirementKindInterfaceReceiver,
			TargetID: target.ID,
			Detail:   fmt.Sprintf("receiver type %s is an interface", target.Receiver.TypeName),
		}
	}
	if target.HasTypeParams && len(target.TypeParams) == 0 {
		return "", &UnsatisfiedRequirement{
			Kind:     UnsatisfiedRequirementKindGenericUnconstrained,
			TargetID: target.ID,
			Detail:   "method has type parameters but no constraints were discovered",
		}
	}
	// Prefer a parameterless same-package constructor when one exists; this
	// matches the receiver planner's priority order (str-hy9b.H5) and gives
	// the method a non-zero receiver state when the package author exposed
	// one. Parameterful constructors are skipped because the wrapper has no
	// way to synthesize their arguments (str-qo1.14).
	for _, c := range ScanConstructors(pkg) {
		if c.TargetType != target.Receiver.TypeName {
			continue
		}
		if len(c.Parameters) > 0 {
			continue
		}
		return wrapper.WrapperKindConstructorPrefix + c.FuncName, nil
	}
	// When the receiver type carries unexported reference-typed fields a
	// constructor is expected to initialize and no parameterless constructor
	// is available, refuse the zero-value fallback: reporting nil-pointer
	// panics for such methods would not reflect real call sites (str-g7h7).
	// Caller short-circuits to OutcomeStatusUnsupported with the kind's
	// detail as short_reason.
	if ReceiverRequiresConstruction(pkg, &target) {
		return "", &UnsatisfiedRequirement{
			Kind:     UnsatisfiedRequirementKindRequiresConstruction,
			TargetID: target.ID,
			Detail:   fmt.Sprintf("receiver type %s requires constructor initialization; no parameterless constructor available", target.Receiver.TypeName),
		}
	}
	// Final fallback mirrors the receiver planner's fallback_zero_value
	// strategy: the wrapper always emits a `zero_value` case for method
	// targets so this token is guaranteed to dispatch. Runtime behavior
	// (whether the zero-value receiver crashes inside the method body)
	// is now surfaced as a real `runtime_failed` outcome with the actual
	// panic message instead of the misleading "unknown receiver kind".
	return wrapper.WrapperKindZeroValue, nil
}

// receiverUnsupportedReason renders an UnsatisfiedRequirement as a
// human-readable short_reason for the unsupported-outcome short-circuit.
func receiverUnsupportedReason(unsat *UnsatisfiedRequirement) string {
	if unsat == nil {
		return "method has no constructible receiver"
	}
	if unsat.Detail != "" {
		return unsat.Detail
	}
	switch unsat.Kind {
	case UnsatisfiedRequirementKindInterfaceReceiver:
		return "method receiver is an interface and cannot be constructed"
	case UnsatisfiedRequirementKindGenericUnconstrained:
		return "method receiver requires generic type arguments that could not be inferred"
	case UnsatisfiedRequirementKindRequiresConstruction:
		return "method receiver requires constructor initialization; no parameterless constructor or hint available"
	default:
		return "method has no constructible receiver"
	}
}

// findFuncDeclByBareName scans every syntax file in pkg for the FuncDecl
// whose name matches `name`. The matcher accepts either the bare AST
// name (e.g. "Write") or the receiver-decorated qualified name shatter-go
// emits for methods (e.g. "(*Foo).Write", str-fuhw.1.1). Free functions
// continue to use the bare form on both sides. When the qualified form
// is supplied for a method the match is unique even when multiple
// methods in the package share a bare name; when the bare form is
// supplied, this returns the first matching FuncDecl in source order
// (preserving prior behavior for callers that have not adopted the
// qualified form yet).
func findFuncDeclByBareName(pkg *packages.Package, name string) *ast.FuncDecl {
	for _, file := range pkg.Syntax {
		if file == nil {
			continue
		}
		for _, decl := range file.Decls {
			fn, ok := decl.(*ast.FuncDecl)
			if !ok || fn.Body == nil {
				continue
			}
			if fn.Name.Name == name || qualifiedNameOf(fn) == name {
				return fn
			}
		}
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

// bestEffortRequestID extracts the integer "id" field from a (possibly
// malformed) JSON request line. Returns 0 when no id can be recovered.
//
// This is intentionally tolerant: it accepts invalid JSON surrounding the
// id field (e.g. trailing garbage after a valid id, missing closing brace)
// so that an error response carrying the original request's id can still
// be aligned with the pending request on the core side. The numeric form
// is the only one accepted; the wire schema specifies id as a uint64.
//
// Recovery is bounded to digits-only after `"id"` plus optional whitespace
// and a colon; any deviation falls back to 0 so we never mis-attribute a
// response to the wrong request.
func bestEffortRequestID(line string) int {
	const idMarker = `"id"`
	idx := strings.Index(line, idMarker)
	if idx < 0 {
		return 0
	}
	rest := line[idx+len(idMarker):]
	// Skip whitespace then a single colon.
	pos := 0
	for pos < len(rest) && (rest[pos] == ' ' || rest[pos] == '\t') {
		pos++
	}
	if pos >= len(rest) || rest[pos] != ':' {
		return 0
	}
	pos++
	for pos < len(rest) && (rest[pos] == ' ' || rest[pos] == '\t') {
		pos++
	}
	digitsStart := pos
	for pos < len(rest) && rest[pos] >= '0' && rest[pos] <= '9' {
		pos++
	}
	if digitsStart == pos {
		return 0
	}
	id, err := strconv.Atoi(rest[digitsStart:pos])
	if err != nil {
		return 0
	}
	return id
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
