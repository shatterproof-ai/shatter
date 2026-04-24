package launcher

import (
	"bufio"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
)

const sessionBufferSize = 4 * 1024 * 1024

// LauncherRequest is the JSON request sent to a running launcher binary.
// Plan selects the invocation strategy; Inputs are the argument values.
type LauncherRequest struct {
	Plan    json.RawMessage   `json:"plan"`
	Inputs  []json.RawMessage `json:"inputs"`
	Capture bool              `json:"capture"`
}

type LauncherSideEffect struct {
	Kind     string          `json:"kind"`
	Level    string          `json:"level,omitempty"`
	Message  string          `json:"message,omitempty"`
	Variable string          `json:"variable,omitempty"`
	Before   json.RawMessage `json:"before,omitempty"`
	After    json.RawMessage `json:"after,omitempty"`
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
	InvocationsDispatched int
}

// OpenSession starts the launcher binary at binaryPath and returns a session
// ready to accept Invoke calls. The caller must call Close when done.
func OpenSession(binaryPath string) (*LauncherSession, error) {
	cmd := exec.Command(binaryPath) //nolint:gosec
	cmd.Stderr = os.Stderr

	stdinPipe, err := cmd.StdinPipe()
	if err != nil {
		return nil, fmt.Errorf("launcher: stdin pipe: %w", err)
	}
	stdoutPipe, err := cmd.StdoutPipe()
	if err != nil {
		stdinPipe.Close()
		return nil, fmt.Errorf("launcher: stdout pipe: %w", err)
	}
	if err := cmd.Start(); err != nil {
		stdinPipe.Close()
		return nil, fmt.Errorf("launcher: start subprocess: %w", err)
	}

	sc := bufio.NewScanner(stdoutPipe)
	sc.Buffer(make([]byte, sessionBufferSize), sessionBufferSize)

	return &LauncherSession{
		cmd:   cmd,
		enc:   json.NewEncoder(stdinPipe),
		sc:    sc,
		stdin: stdinPipe,
	}, nil
}

// Invoke sends one request to the launcher binary and returns the response.
// InvocationsDispatched is incremented on every successful round-trip.
func (s *LauncherSession) Invoke(req LauncherRequest) (LauncherResponse, error) {
	if err := s.enc.Encode(req); err != nil {
		return LauncherResponse{}, fmt.Errorf("launcher: send request: %w", err)
	}
	if !s.sc.Scan() {
		if err := s.sc.Err(); err != nil {
			return LauncherResponse{}, fmt.Errorf("launcher: read response: %w", err)
		}
		return LauncherResponse{}, fmt.Errorf("launcher: subprocess exited unexpectedly")
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
	err := s.cmd.Wait()
	var exitErr *exec.ExitError
	if errors.As(err, &exitErr) && exitErr.ExitCode() == 1 {
		return nil
	}
	return err
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
	return s.cmd.Wait()
}
