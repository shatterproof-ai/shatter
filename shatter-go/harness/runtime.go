// Package harness provides the shared runtime for Shatter's Go harness binaries.
//
// Per-function harness main.go files import this package and call [RunLoop] with
// a handler closure. The package provides JSON protocol I/O, console capture,
// performance measurement, panic recovery, and side-effect helpers.
//
// Recorder-specific types (branch decisions, scope events) are represented as
// [json.RawMessage] in [Response] so the package has no dependency on
// per-function generated code.
package harness

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"runtime"
	"strings"
	"time"
)

// stdinBufferSize is the maximum line size for stdin/stdout JSON messages (4 MB).
const stdinBufferSize = 4 * 1024 * 1024

// panicStackSize is the buffer size for capturing goroutine stacks on panic.
const panicStackSize = 4096

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

// Request is the JSON request sent to the harness loop per execution.
type Request struct {
	Plan    json.RawMessage   `json:"plan,omitempty"`
	Inputs  []json.RawMessage `json:"inputs"`
	Capture bool              `json:"capture"`
}

// SideEffect represents a single observed side effect during execution.
type SideEffect struct {
	Kind     string          `json:"kind"`
	Level    string          `json:"level,omitempty"`
	Message  string          `json:"message,omitempty"`
	Variable string          `json:"variable,omitempty"`
	Before   json.RawMessage `json:"before,omitempty"`
	After    json.RawMessage `json:"after,omitempty"`
}

// Error describes a runtime error or panic captured during execution.
type Error struct {
	ErrorType     string `json:"error_type"`
	Message       string `json:"message"`
	Stack         string `json:"stack,omitempty"`
	ErrorCategory string `json:"error_category,omitempty"`
}

// Perf holds performance metrics from a single execution.
type Perf struct {
	WallTimeMs         float64 `json:"wall_time_ms"`
	CPUTimeUs          int64   `json:"cpu_time_us"`
	HeapUsedBytes      int64   `json:"heap_used_bytes"`
	HeapAllocatedBytes int64   `json:"heap_allocated_bytes"`
}

// Response is the JSON response written by the harness loop per execution.
//
// BranchPath, LinesExecuted, ScopeEvents, and ExternalCalls are
// [json.RawMessage] so the runtime has no dependency on per-function recorder
// types. The generated main.go marshals recorder-specific structs into these
// fields before returning the response.
type Response struct {
	ReturnValue   json.RawMessage `json:"return_value,omitempty"`
	BranchPath    json.RawMessage `json:"branch_path"`
	LinesExecuted json.RawMessage `json:"lines_executed"`
	ScopeEvents   json.RawMessage `json:"scope_events"`
	SideEffects   []SideEffect    `json:"side_effects"`
	ExternalCalls json.RawMessage `json:"external_calls,omitempty"`
	ThrownError   *Error          `json:"thrown_error,omitempty"`
	Performance   *Perf           `json:"performance"`
	Error         string          `json:"error,omitempty"`
}

// ---------------------------------------------------------------------------
// Harness loop
// ---------------------------------------------------------------------------

// RunLoop reads JSON [Request] lines from stdin, passes each to handler, and
// writes the [Response] as JSON to stdout. It exits with code 1 on EOF or
// read error, which signals the parent process that the harness is done.
func RunLoop(handler func(Request) Response) {
	sc := bufio.NewScanner(os.Stdin)
	sc.Buffer(make([]byte, stdinBufferSize), stdinBufferSize)
	enc := json.NewEncoder(os.Stdout)

	for sc.Scan() {
		var req Request
		if err := json.Unmarshal(sc.Bytes(), &req); err != nil {
			_ = enc.Encode(Response{Error: "bad request: " + err.Error()})
			continue
		}
		resp := handler(req)
		_ = enc.Encode(resp)
	}
	os.Exit(1)
}

// ---------------------------------------------------------------------------
// Console capture
// ---------------------------------------------------------------------------

// Capture redirects os.Stdout and os.Stderr to pipes so that fmt.Print* calls
// from the target function are captured rather than mixing with JSON responses
// on stdout. Call [CaptureConsole] to start and [Capture.Stop] to restore.
type Capture struct {
	origOut *os.File
	origErr *os.File
	wOut    *os.File
	wErr    *os.File
	capOut  *bytes.Buffer
	capErr  *bytes.Buffer
	donOut  chan struct{}
	donErr  chan struct{}
}

// CaptureConsole starts redirecting os.Stdout and os.Stderr. The returned
// [Capture] must be stopped via [Capture.Stop] to restore the originals.
func CaptureConsole() *Capture {
	c := &Capture{}
	var rOut, rErr *os.File

	rOut, c.wOut, _ = os.Pipe()
	c.origOut = os.Stdout
	os.Stdout = c.wOut
	c.capOut = &bytes.Buffer{}
	c.donOut = make(chan struct{})
	go func() { _, _ = io.Copy(c.capOut, rOut); close(c.donOut) }()

	rErr, c.wErr, _ = os.Pipe()
	c.origErr = os.Stderr
	os.Stderr = c.wErr
	c.capErr = &bytes.Buffer{}
	c.donErr = make(chan struct{})
	go func() { _, _ = io.Copy(c.capErr, rErr); close(c.donErr) }()

	return c
}

// Stop restores the original stdout/stderr and returns the captured text.
func (c *Capture) Stop() (stdout, stderr string) {
	os.Stdout = c.origOut
	c.wOut.Close()
	<-c.donOut
	os.Stderr = c.origErr
	c.wErr.Close()
	<-c.donErr
	return c.capOut.String(), c.capErr.String()
}

// ---------------------------------------------------------------------------
// Performance measurement
// ---------------------------------------------------------------------------

// PerfSnap captures an initial performance snapshot. Call [StartPerf] to create
// one, then [PerfSnap.Finish] after execution to compute deltas.
type PerfSnap struct {
	memBefore runtime.MemStats
	start     time.Time
}

// StartPerf captures the initial memory stats and wall clock time.
func StartPerf() *PerfSnap {
	s := &PerfSnap{}
	runtime.ReadMemStats(&s.memBefore)
	s.start = time.Now()
	return s
}

// Finish computes performance deltas and returns the result.
func (s *PerfSnap) Finish() *Perf {
	elapsed := time.Since(s.start)
	var memAfter runtime.MemStats
	runtime.ReadMemStats(&memAfter)
	return &Perf{
		WallTimeMs:         float64(elapsed.Microseconds()) / 1000.0,
		CPUTimeUs:          elapsed.Microseconds(),
		HeapUsedBytes:      int64(memAfter.HeapInuse) - int64(s.memBefore.HeapInuse),
		HeapAllocatedBytes: int64(memAfter.TotalAlloc) - int64(s.memBefore.TotalAlloc),
	}
}

// ---------------------------------------------------------------------------
// Panic recovery
// ---------------------------------------------------------------------------

// SafeCall executes fn inside a deferred recover(). Returns nil on success,
// or a populated [*Error] on panic.
func SafeCall(fn func()) *Error {
	var caught *Error
	func() {
		defer func() {
			if r := recover(); r != nil {
				stk := make([]byte, panicStackSize)
				n := runtime.Stack(stk, false)
				caught = &Error{
					ErrorType:     "panic",
					Message:       fmt.Sprintf("%v", r),
					Stack:         string(stk[:n]),
					ErrorCategory: "runtime",
				}
			}
		}()
		fn()
	}()
	return caught
}

// ---------------------------------------------------------------------------
// Side-effect helpers
// ---------------------------------------------------------------------------

// ConsoleSideEffects builds [SideEffect] entries from captured stdout/stderr.
// Empty strings are ignored.
func ConsoleSideEffects(capturedOut, capturedErr string) []SideEffect {
	var se []SideEffect
	if s := strings.TrimSpace(capturedOut); s != "" {
		se = append(se, SideEffect{Kind: "console_output", Level: "log", Message: s})
	}
	if s := strings.TrimSpace(capturedErr); s != "" {
		se = append(se, SideEffect{Kind: "console_output", Level: "error", Message: s})
	}
	return se
}
