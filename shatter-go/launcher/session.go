package launcher

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"os/exec"
	"strings"
	"sync"
	"time"

	"github.com/shatter-dev/shatter/shatter-go/sandbox"
)

const sessionBufferSize = 4 * 1024 * 1024

// stderrCaptureLimit caps how much subprocess stderr is retained for
// diagnostic surfacing. The cap protects against runaway subprocesses
// that flood stderr; the tail is what callers see when classifying a
// crash, and 16 KiB is enough to capture Go panic traces and typical
// launcher complaint messages while bounding memory.
const stderrCaptureLimit = 16 * 1024

// LauncherRequest is the JSON request sent to a running launcher binary.
// Plan selects the invocation strategy; Inputs are the argument values.
type LauncherRequest struct {
	Plan    json.RawMessage   `json:"plan"`
	Inputs  []json.RawMessage `json:"inputs"`
	Capture bool              `json:"capture"`
}

type LauncherSideEffect struct {
	Kind      string           `json:"kind"`
	Level     string           `json:"level,omitempty"`
	Message   string           `json:"message,omitempty"`
	Path      string           `json:"path,omitempty"`
	Content   string           `json:"content,omitempty"`
	Method    string           `json:"method,omitempty"`
	URL       string           `json:"url,omitempty"`
	Body      *json.RawMessage `json:"body,omitempty"`
	Name      string           `json:"name,omitempty"`
	ErrorType string           `json:"error_type,omitempty"`
	Stack     *string          `json:"stack,omitempty"`
	Variable  string           `json:"variable,omitempty"`
	Value     *string          `json:"value,omitempty"`
	Before    json.RawMessage  `json:"before,omitempty"`
	After     json.RawMessage  `json:"after,omitempty"`
}

type LauncherError struct {
	ErrorType     string `json:"error_type"`
	Message       string `json:"message"`
	Stack         string `json:"stack,omitempty"`
	ErrorCategory string `json:"error_category,omitempty"`
}

type LauncherPerf struct {
	WallTimeMs         float64 `json:"wall_time_ms"`
	CPUTimeUs          int64   `json:"cpu_time_us"`
	HeapUsedBytes      int64   `json:"heap_used_bytes"`
	HeapAllocatedBytes int64   `json:"heap_allocated_bytes"`
}

// LauncherResponse is the JSON response from a running launcher binary.
type LauncherResponse struct {
	ReturnValue   json.RawMessage      `json:"return_value,omitempty"`
	BranchPath    json.RawMessage      `json:"branch_path"`
	LinesExecuted json.RawMessage      `json:"lines_executed"`
	ScopeEvents   json.RawMessage      `json:"scope_events"`
	ExternalCalls json.RawMessage      `json:"external_calls,omitempty"`
	SideEffects   []LauncherSideEffect `json:"side_effects"`
	ThrownError   *LauncherError       `json:"thrown_error,omitempty"`
	Performance   *LauncherPerf        `json:"performance,omitempty"`
	Error         string               `json:"error,omitempty"`
}

// LauncherSession manages a running launcher binary subprocess. Invoke sends
// individual requests over a persistent stdin/stdout pipe; the binary handles
// all requests in a single process lifetime.
//
// InvocationsDispatched is incremented for every request that receives a
// response (error responses are counted; transport failures are not).
type LauncherSession struct {
	cmd                   *exec.Cmd
	enc                   *json.Encoder
	sc                    *bufio.Scanner
	stdin                 interface{ Close() error }
	cleanup               func() error
	stderr                *capturedStderr
	binaryPath            string
	waitOnce              sync.Once
	waitErr               error
	InvocationsDispatched int
}

// capturedStderr is a bounded io.Writer that buffers up to limit bytes of
// subprocess stderr for diagnostic reporting. Writes beyond the cap are
// discarded silently so the subprocess is never blocked by a full pipe.
type capturedStderr struct {
	mu    sync.Mutex
	buf   []byte
	limit int
}

func (c *capturedStderr) Write(p []byte) (int, error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	remaining := c.limit - len(c.buf)
	if remaining > 0 {
		if len(p) <= remaining {
			c.buf = append(c.buf, p...)
		} else {
			c.buf = append(c.buf, p[:remaining]...)
		}
	}
	return len(p), nil
}

func (c *capturedStderr) String() string {
	c.mu.Lock()
	defer c.mu.Unlock()
	return string(c.buf)
}

// SessionOptions configures launcher subprocess execution.
type SessionOptions struct {
	ProjectRoot string
	WorkDir     string
	Env         []string
	Sandbox     sandbox.Runner
}

// OpenSession starts the launcher binary at binaryPath and returns a session
// ready to accept Invoke calls. The caller must call Close when done.
func OpenSession(binaryPath string) (*LauncherSession, error) {
	return OpenSessionWithOptions(binaryPath, SessionOptions{})
}

// OpenSessionWithOptions starts the launcher binary with explicit execution
// options. When options.Sandbox is enabled, the launcher runs behind that
// backend with ProjectRoot mounted as a disposable scratch filesystem.
func OpenSessionWithOptions(binaryPath string, options SessionOptions) (*LauncherSession, error) {
	prepared, err := options.Sandbox.Command(sandbox.Spec{
		BinaryPath:  binaryPath,
		ProjectRoot: options.ProjectRoot,
		WorkDir:     options.WorkDir,
		Env:         options.Env,
	})
	if err != nil {
		return nil, fmt.Errorf("launcher: prepare subprocess: %w", err)
	}
	cleanup := prepared.Cleanup
	cmd := prepared.Cmd
	stderrBuf := &capturedStderr{limit: stderrCaptureLimit}
	cmd.Stderr = stderrBuf

	stdinPipe, err := cmd.StdinPipe()
	if err != nil {
		_ = cleanup()
		return nil, fmt.Errorf("launcher: stdin pipe: %w", err)
	}
	stdoutPipe, err := cmd.StdoutPipe()
	if err != nil {
		stdinPipe.Close()
		_ = cleanup()
		return nil, fmt.Errorf("launcher: stdout pipe: %w", err)
	}
	if err := cmd.Start(); err != nil {
		stdinPipe.Close()
		_ = cleanup()
		return nil, fmt.Errorf("launcher: start subprocess: %w", err)
	}

	sc := bufio.NewScanner(stdoutPipe)
	sc.Buffer(make([]byte, sessionBufferSize), sessionBufferSize)

	return &LauncherSession{
		cmd:        cmd,
		enc:        json.NewEncoder(stdinPipe),
		sc:         sc,
		stdin:      stdinPipe,
		cleanup:    cleanup,
		stderr:     stderrBuf,
		binaryPath: binaryPath,
	}, nil
}

// wait collects the subprocess exit status exactly once. Subsequent calls
// return the same error so subprocessExitError and Close cannot race.
func (s *LauncherSession) wait() error {
	s.waitOnce.Do(func() { s.waitErr = s.cmd.Wait() })
	return s.waitErr
}

// subprocessExitError builds the diagnostic returned when the subprocess
// exits before producing a response. The message includes the binary path,
// the exit status, and the tail of captured stderr so callers (and the
// failureOutcome classifier in protocol/handler.go) can render a structured
// non-panic diagnostic instead of an opaque "subprocess exited unexpectedly"
// (str-jeen.80).
func (s *LauncherSession) subprocessExitError() error {
	exitErr := s.wait()
	exitStatus := "exit status 0"
	if exitErr != nil {
		exitStatus = exitErr.Error()
	}
	stderr := strings.TrimRight(s.stderr.String(), "\n")
	if stderr == "" {
		return fmt.Errorf("launcher: subprocess exited unexpectedly: %s: %s", s.binaryPath, exitStatus)
	}
	return fmt.Errorf("launcher: subprocess exited unexpectedly: %s: %s\nstderr: %s", s.binaryPath, exitStatus, stderr)
}

// Invoke sends one request to the launcher binary and returns the response.
// InvocationsDispatched is incremented on every successful round-trip.
func (s *LauncherSession) Invoke(req LauncherRequest) (LauncherResponse, error) {
	return s.InvokeWithTimeout(req, 0)
}

// InvokeWithTimeout sends one request and races the response read against the
// supplied timeout. A non-positive timeout disables the timer and blocks
// indefinitely. On timeout the subprocess is killed and the returned error
// message contains "timed out" so it flows through failureOutcome as
// OutcomeStatusTimedOut.
func (s *LauncherSession) InvokeWithTimeout(req LauncherRequest, timeout time.Duration) (LauncherResponse, error) {
	if err := s.enc.Encode(req); err != nil {
		return LauncherResponse{}, fmt.Errorf("launcher: send request: %w", err)
	}

	type scanResult struct {
		ok  bool
		err error
	}
	done := make(chan scanResult, 1)
	go func() {
		ok := s.sc.Scan()
		done <- scanResult{ok: ok, err: s.sc.Err()}
	}()

	var timer *time.Timer
	var timerC <-chan time.Time
	if timeout > 0 {
		timer = time.NewTimer(timeout)
		timerC = timer.C
		defer timer.Stop()
	}

	select {
	case r := <-done:
		if !r.ok {
			if r.err != nil {
				return LauncherResponse{}, fmt.Errorf("launcher: read response: %w", r.err)
			}
			return LauncherResponse{}, s.subprocessExitError()
		}
	case <-timerC:
		if s.cmd.Process != nil {
			_ = s.cmd.Process.Kill()
		}
		cleanupErr := s.runCleanup()
		<-done
		_ = s.wait()
		if cleanupErr != nil {
			return LauncherResponse{}, fmt.Errorf("launcher: execution timed out after %s: cleanup: %w", timeout, cleanupErr)
		}
		return LauncherResponse{}, fmt.Errorf("launcher: execution timed out after %s", timeout)
	}

	var resp LauncherResponse
	if err := json.Unmarshal(s.sc.Bytes(), &resp); err != nil {
		return LauncherResponse{}, fmt.Errorf("launcher: decode response: %w", err)
	}
	s.InvocationsDispatched++
	return resp, nil
}

// Close shuts down the launcher subprocess. Closing the stdin pipe signals the
// binary to exit; Wait collects the exit status.
func (s *LauncherSession) Close() error {
	_ = s.stdin.Close()
	err := s.wait()
	cleanupErr := s.runCleanup()
	var exitErr *exec.ExitError
	if errors.As(err, &exitErr) && exitErr.ExitCode() == 1 {
		return cleanupErr
	}
	if err != nil {
		return err
	}
	return cleanupErr
}

// Kill forcibly terminates the launcher subprocess. Intended for recovery
// tests that verify a dead session is respawned on the next execute.
func (s *LauncherSession) Kill() error {
	if s.cmd.Process == nil {
		return nil
	}
	if err := s.cmd.Process.Kill(); err != nil {
		return err
	}
	err := s.wait()
	cleanupErr := s.runCleanup()
	if err != nil {
		return err
	}
	return cleanupErr
}

func (s *LauncherSession) runCleanup() error {
	if s.cleanup == nil {
		return nil
	}
	return s.cleanup()
}
