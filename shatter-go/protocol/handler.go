package protocol

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
)

const frontendVersion = "0.1.0"
const frontendLanguage = "go"

// logLevelRank maps level names to numeric ranks for comparison.
var logLevelRank = map[string]int{
	"error": 0,
	"warn":  1,
	"info":  2,
	"debug": 3,
	"trace": 4,
}

// getLogLevel reads SHATTER_LOG_LEVEL from environment, defaulting to "info".
func getLogLevel() string {
	level := strings.ToLower(os.Getenv("SHATTER_LOG_LEVEL"))
	if _, ok := logLevelRank[level]; ok {
		return level
	}
	return "info"
}

// Handler processes protocol requests and writes responses.
type Handler struct {
	reader           *bufio.Scanner
	writer           io.Writer
	log              io.Writer
	logLevel         string
	lastAnalyzedFile string // remembered from the most recent analyze command
}

// NewHandler creates a handler reading from r, writing responses to w,
// and logging debug output to logw.
func NewHandler(r io.Reader, w io.Writer, logw io.Writer) *Handler {
	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 0, 1024*1024), 10*1024*1024) // 10MB max line
	return &Handler{
		reader:   scanner,
		writer:   w,
		log:      logw,
		logLevel: getLogLevel(),
	}
}

// NewHandlerWithLogLevel creates a handler with an explicit log level (for testing).
func NewHandlerWithLogLevel(r io.Reader, w io.Writer, logw io.Writer, level string) *Handler {
	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 0, 1024*1024), 10*1024*1024)
	return &Handler{
		reader:   scanner,
		writer:   w,
		log:      logw,
		logLevel: level,
	}
}

// Run processes requests until shutdown or EOF. Returns nil on clean shutdown.
func (h *Handler) Run() error {
	h.logAt("debug", "Starting Go frontend (protocol %s)", ProtocolVersion)

	for h.reader.Scan() {
		line := h.reader.Text()
		if line == "" {
			continue
		}

		h.logf("Received: %s", line)

		var req Request
		if err := json.Unmarshal([]byte(line), &req); err != nil {
			h.logf("Failed to parse request: %v", err)
			continue
		}

		resp, shutdown := h.dispatch(req)
		if err := h.send(resp); err != nil {
			return fmt.Errorf("writing response: %w", err)
		}

		if shutdown {
			h.logAt("debug", "Shutting down")
			return nil
		}
	}

	if err := h.reader.Err(); err != nil {
		return fmt.Errorf("reading stdin: %w", err)
	}

	h.logAt("debug", "Stdin closed, exiting")
	return nil
}

func (h *Handler) dispatch(req Request) (Response, bool) {
	base := Response{
		ProtocolVersion: ProtocolVersion,
		ID:              req.ID,
	}

	if req.ProtocolVersion != ProtocolVersion {
		base.Status = "error"
		base.Code = "version_mismatch"
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
		base.Code = "invalid_request"
		base.Message = fmt.Sprintf("unknown command: %s", req.Command)
		return base, false
	}
}

func (h *Handler) handleHandshake(resp Response, req Request) Response {
	resp.Status = "handshake"
	resp.FrontendVersion = frontendVersion
	resp.Language = frontendLanguage
	resp.Capabilities = []string{
		"analyze", "execute", "instrument",
		"complex_type:date", "complex_type:duration", "complex_type:url",
		"complex_type:reg_exp", "complex_type:ip_address", "complex_type:big_int",
		"complex_type:rational", "complex_type:big_decimal", "complex_type:error",
	}
	return resp
}

func (h *Handler) handleAnalyze(resp Response, req Request) Response {
	if req.File == "" {
		resp.Status = "error"
		resp.Code = "invalid_request"
		resp.Message = "analyze command requires a file path"
		return resp
	}

	if _, err := os.Stat(req.File); err != nil {
		resp.Status = "error"
		resp.Code = "file_not_found"
		resp.Message = fmt.Sprintf("file not found: %s", req.File)
		return resp
	}

	h.lastAnalyzedFile = req.File

	var functionName string
	if req.Function != nil {
		functionName = *req.Function
	}

	functions, err := AnalyzeFile(req.File, functionName)
	if err != nil {
		if functionName != "" && isNotFound(err) {
			resp.Status = "error"
			resp.Code = "function_not_found"
			resp.Message = fmt.Sprintf("function %q not found in %s", functionName, req.File)
			return resp
		}
		resp.Status = "error"
		resp.Code = "parse_error"
		resp.Message = err.Error()
		return resp
	}

	resp.Status = "analyze"
	resp.Functions = functions
	return resp
}

func isNotFound(err error) bool {
	return err != nil && strings.HasPrefix(err.Error(), "function not found")
}

func (h *Handler) handleInstrument(resp Response, req Request) Response {
	if req.File == "" {
		resp.Status = "error"
		resp.Code = "invalid_request"
		resp.Message = "instrument command requires a file path"
		return resp
	}

	if _, err := os.Stat(req.File); err != nil {
		resp.Status = "error"
		resp.Code = "file_not_found"
		resp.Message = fmt.Sprintf("file not found: %s", req.File)
		return resp
	}

	outputDir, err := instrument.InstrumentFile(req.File, req.Function)
	if err != nil {
		resp.Status = "error"
		resp.Code = "internal_error"
		resp.Message = fmt.Sprintf("instrumentation failed: %v", err)
		return resp
	}

	instrumented := true
	resp.Status = "instrument"
	resp.Instrumented = &instrumented
	resp.OutputFile = &outputDir
	return resp
}

func (h *Handler) handleExecute(resp Response, req Request) Response {
	file := req.File
	if file == "" {
		file = h.lastAnalyzedFile
	}
	if file == "" {
		resp.Status = "error"
		resp.Code = "invalid_request"
		resp.Message = "execute command requires a file path (or a prior analyze)"
		return resp
	}

	if req.Function == nil || *req.Function == "" {
		resp.Status = "error"
		resp.Code = "invalid_request"
		resp.Message = "execute command requires a function name"
		return resp
	}

	if _, err := os.Stat(file); err != nil {
		resp.Status = "error"
		resp.Code = "file_not_found"
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
	result, err := instrument.ExecuteFunction(file, *req.Function, req.Inputs, execMocks)
	if err != nil {
		resp.Status = "error"
		if strings.Contains(err.Error(), "function not found") {
			resp.Code = "function_not_found"
		} else if strings.Contains(err.Error(), "build failed") {
			resp.Code = "instrumentation_failed"
		} else if strings.Contains(err.Error(), "timed out") {
			resp.Code = "execution_timeout"
		} else {
			resp.Code = "internal_error"
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
	resp.CallsToExternal = []ExternalCall{}
	resp.SideEffects = []SideEffect{}
	resp.Performance = &PerfMetrics{
		WallTimeMs:         result.Performance.WallTimeMs,
		CPUTimeUs:          result.Performance.CPUTimeUs,
		HeapUsedBytes:      result.Performance.HeapUsedBytes,
		HeapAllocatedBytes: result.Performance.HeapAllocatedBytes,
	}

	return resp
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
		var constraint *SymConstraint
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

func (h *Handler) handleSetup(resp Response, req Request) Response {
	resp.Status = "error"
	resp.Code = "internal_error"
	resp.Message = "setup command not yet implemented"
	return resp
}

func (h *Handler) handleTeardown(resp Response, req Request) Response {
	resp.Status = "error"
	resp.Code = "internal_error"
	resp.Message = "teardown command not yet implemented"
	return resp
}

func (h *Handler) handleGenerate(resp Response, req Request) Response {
	resp.Status = "error"
	resp.Code = "internal_error"
	resp.Message = "generate command not yet implemented"
	return resp
}

func (h *Handler) handleShutdown(resp Response) Response {
	resp.Status = "shutdown_ack"
	return resp
}

func (h *Handler) send(resp Response) error {
	data, err := json.Marshal(resp)
	if err != nil {
		return fmt.Errorf("marshaling response: %w", err)
	}
	line := string(data) + "\n"
	h.logf("Sent: %s", string(data))
	_, err = io.WriteString(h.writer, line)
	return err
}

func (h *Handler) logf(format string, args ...any) {
	h.logAt("trace", format, args...)
}

func (h *Handler) logAt(level string, format string, args ...any) {
	if logLevelRank[h.logLevel] >= logLevelRank[level] {
		fmt.Fprintf(h.log, "[shatter-go] "+format+"\n", args...)
	}
}
