package protocol

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
)

const frontendVersion = "0.1.0"
const frontendLanguage = "go"

// Handler processes protocol requests and writes responses.
type Handler struct {
	reader *bufio.Scanner
	writer io.Writer
	log    io.Writer
}

// NewHandler creates a handler reading from r, writing responses to w,
// and logging debug output to logw.
func NewHandler(r io.Reader, w io.Writer, logw io.Writer) *Handler {
	scanner := bufio.NewScanner(r)
	scanner.Buffer(make([]byte, 0, 1024*1024), 10*1024*1024) // 10MB max line
	return &Handler{
		reader: scanner,
		writer: w,
		log:    logw,
	}
}

// Run processes requests until shutdown or EOF. Returns nil on clean shutdown.
func (h *Handler) Run() error {
	h.logf("Starting Go frontend (protocol %s)", ProtocolVersion)

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
			h.logf("Shutting down")
			return nil
		}
	}

	if err := h.reader.Err(); err != nil {
		return fmt.Errorf("reading stdin: %w", err)
	}

	h.logf("Stdin closed, exiting")
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
	resp.Capabilities = []string{"analyze", "execute", "instrument"}
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

	// Stub: real analysis will use go/types and go/ast.
	resp.Status = "analyze"
	resp.Functions = []FunctionAnalysis{}
	return resp
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
	// Stub: real execution will compile and run instrumented code.
	resp.Status = "error"
	resp.Code = "internal_error"
	resp.Message = "execute command not yet implemented"
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
	fmt.Fprintf(h.log, "[shatter-go] "+format+"\n", args...)
}
