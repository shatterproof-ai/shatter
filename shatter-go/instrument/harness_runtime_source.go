package instrument

// harnessRuntimeSource returns the Go source code for the shared harness
// runtime package. This source is written to a cache directory once per
// process and referenced via a replace directive in each harness's go.mod.
//
// The returned code must be kept in sync with shatter-go/harness/runtime.go.
// Any behavioral change to the harness runtime must be reflected in both
// locations. The canonical version lives in shatter-go/harness/; this
// function is the embedded copy for deployment without source access.
func harnessRuntimeSource() string {
	return `package harness

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

const stdinBufferSize = 4 * 1024 * 1024
const panicStackSize = 4096

type Request struct {
	Inputs  []json.RawMessage ` + "`" + `json:"inputs"` + "`" + `
	Capture bool              ` + "`" + `json:"capture"` + "`" + `
}

type SideEffect struct {
	Kind     string          ` + "`" + `json:"kind"` + "`" + `
	Level    string          ` + "`" + `json:"level,omitempty"` + "`" + `
	Message  string          ` + "`" + `json:"message,omitempty"` + "`" + `
	Variable string          ` + "`" + `json:"variable,omitempty"` + "`" + `
	Before   json.RawMessage ` + "`" + `json:"before,omitempty"` + "`" + `
	After    json.RawMessage ` + "`" + `json:"after,omitempty"` + "`" + `
}

type Error struct {
	ErrorType     string ` + "`" + `json:"error_type"` + "`" + `
	Message       string ` + "`" + `json:"message"` + "`" + `
	Stack         string ` + "`" + `json:"stack,omitempty"` + "`" + `
	ErrorCategory string ` + "`" + `json:"error_category,omitempty"` + "`" + `
}

type Perf struct {
	WallTimeMs         float64 ` + "`" + `json:"wall_time_ms"` + "`" + `
	CPUTimeUs          int64   ` + "`" + `json:"cpu_time_us"` + "`" + `
	HeapUsedBytes      int64   ` + "`" + `json:"heap_used_bytes"` + "`" + `
	HeapAllocatedBytes int64   ` + "`" + `json:"heap_allocated_bytes"` + "`" + `
}

type Response struct {
	ReturnValue   json.RawMessage ` + "`" + `json:"return_value,omitempty"` + "`" + `
	BranchPath    json.RawMessage ` + "`" + `json:"branch_path"` + "`" + `
	LinesExecuted json.RawMessage ` + "`" + `json:"lines_executed"` + "`" + `
	ScopeEvents   json.RawMessage ` + "`" + `json:"scope_events"` + "`" + `
	SideEffects   []SideEffect    ` + "`" + `json:"side_effects"` + "`" + `
	ExternalCalls json.RawMessage ` + "`" + `json:"external_calls,omitempty"` + "`" + `
	ThrownError   *Error          ` + "`" + `json:"thrown_error,omitempty"` + "`" + `
	Performance   *Perf           ` + "`" + `json:"performance"` + "`" + `
	Error         string          ` + "`" + `json:"error,omitempty"` + "`" + `
}

func RunLoop(handler func(Request) Response) {
	sc := bufio.NewScanner(os.Stdin)
	sc.Buffer(make([]byte, stdinBufferSize), stdinBufferSize)
	enc := json.NewEncoder(os.Stdout)
	for sc.Scan() {
		var req Request
		if err := json.Unmarshal(sc.Bytes(), &req); err != nil {
			enc.Encode(Response{Error: "bad request: " + err.Error()})
			continue
		}
		resp := handler(req)
		enc.Encode(resp)
	}
	os.Exit(1)
}

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

func CaptureConsole() *Capture {
	c := &Capture{}
	var rOut, rErr *os.File
	rOut, c.wOut, _ = os.Pipe()
	c.origOut = os.Stdout
	os.Stdout = c.wOut
	c.capOut = &bytes.Buffer{}
	c.donOut = make(chan struct{})
	go func() { io.Copy(c.capOut, rOut); close(c.donOut) }()
	rErr, c.wErr, _ = os.Pipe()
	c.origErr = os.Stderr
	os.Stderr = c.wErr
	c.capErr = &bytes.Buffer{}
	c.donErr = make(chan struct{})
	go func() { io.Copy(c.capErr, rErr); close(c.donErr) }()
	return c
}

func (c *Capture) Stop() (stdout, stderr string) {
	os.Stdout = c.origOut
	c.wOut.Close()
	<-c.donOut
	os.Stderr = c.origErr
	c.wErr.Close()
	<-c.donErr
	return c.capOut.String(), c.capErr.String()
}

type PerfSnap struct {
	memBefore runtime.MemStats
	start     time.Time
}

func StartPerf() *PerfSnap {
	s := &PerfSnap{}
	runtime.ReadMemStats(&s.memBefore)
	s.start = time.Now()
	return s
}

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
`
}
