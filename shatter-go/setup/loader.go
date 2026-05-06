// Package setup provides setup file loading and context management for the
// Go frontend. Setup files are compiled and run as subprocesses; their JSON
// stdout output becomes the opaque setup_context returned to the core engine.
package setup

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"sync"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
	"github.com/shatter-dev/shatter/shatter-go/sandbox"
)

// contextKey uniquely identifies a setup context by scope and level.
type contextKey struct {
	Scope string
	Level string
}

// Loader compiles and runs Go setup files, caching their contexts by
// (scope, level) for later teardown.
type Loader struct {
	mu       sync.Mutex
	contexts map[contextKey]json.RawMessage
}

// NewLoader creates a Loader with an empty context cache.
func NewLoader() *Loader {
	return &Loader{
		contexts: make(map[contextKey]json.RawMessage),
	}
}

// RunSetup compiles and executes the setup file, returning the opaque context.
// The setup file must be a valid Go source file with a main function that
// prints a single JSON object to stdout. Environment variables are set to
// communicate scope, level, and parent context to the setup process.
func (l *Loader) RunSetup(file, scope, level string, projectRoot *string, parentContext json.RawMessage) (json.RawMessage, error) {
	absFile, err := filepath.Abs(file)
	if err != nil {
		return nil, fmt.Errorf("resolving setup file path: %w", err)
	}
	if _, err := os.Stat(absFile); err != nil {
		return nil, fmt.Errorf("setup file not found: %s", absFile)
	}

	// Build the setup file to a temporary binary.
	tmpDir, err := os.MkdirTemp("", "shatter-setup-*")
	if err != nil {
		return nil, fmt.Errorf("creating temp dir: %w", err)
	}
	defer os.RemoveAll(tmpDir)

	binPath := filepath.Join(tmpDir, "setup")
	// `-buildvcs=false` disables VCS stamping for the disposable setup
	// binary. Without it, `-buildvcs=auto` fails with
	// `error obtaining VCS status` when the setup file lives inside a
	// module checkout the toolchain cannot probe. See str-qo1.15.
	buildCmd := exec.Command("go", "build", "-buildvcs=false", "-o", binPath, absFile)
	buildCmd.Dir = filepath.Dir(absFile)
	if env := instrument.WorkspaceGoEnv(); env != nil {
		buildCmd.Env = env
	}
	if out, err := buildCmd.CombinedOutput(); err != nil {
		return nil, fmt.Errorf("building setup file: %s: %w", string(out), err)
	}

	// Run the compiled setup binary.
	runEnv := append(os.Environ(),
		"SHATTER_SETUP_SCOPE="+scope,
		"SHATTER_SETUP_LEVEL="+level,
	)
	if projectRoot != nil {
		runEnv = append(runEnv, "SHATTER_PROJECT_ROOT="+*projectRoot)
	}
	if len(parentContext) > 0 {
		runEnv = append(runEnv, "SHATTER_PARENT_CONTEXT="+string(parentContext))
	}

	runProjectRoot := filepath.Dir(absFile)
	if projectRoot != nil && *projectRoot != "" {
		runProjectRoot = *projectRoot
	}
	prepared, err := sandbox.FromEnv().Command(sandbox.Spec{
		BinaryPath:  binPath,
		ProjectRoot: runProjectRoot,
		WorkDir:     filepath.Dir(absFile),
		Env:         runEnv,
	})
	if err != nil {
		return nil, fmt.Errorf("preparing setup sandbox: %w", err)
	}
	defer func() { _ = prepared.Cleanup() }()

	stdout, err := prepared.Cmd.Output()
	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			return nil, fmt.Errorf("setup file exited with error: %s: %s", exitErr, string(exitErr.Stderr))
		}
		return nil, fmt.Errorf("running setup file: %w", err)
	}

	// Validate that the output is valid JSON.
	stdout = trimTrailingNewlines(stdout)
	if len(stdout) == 0 {
		// No output — use empty object as context.
		stdout = []byte("{}")
	}
	if !json.Valid(stdout) {
		return nil, fmt.Errorf("setup file output is not valid JSON: %s", string(stdout))
	}

	ctx := json.RawMessage(stdout)

	// Cache the context.
	l.mu.Lock()
	l.contexts[contextKey{Scope: scope, Level: level}] = ctx
	l.mu.Unlock()

	return ctx, nil
}

// Teardown removes the cached context for the given scope and level.
// Returns true if a context was found and removed, false otherwise.
func (l *Loader) Teardown(scope, level string) bool {
	l.mu.Lock()
	defer l.mu.Unlock()
	key := contextKey{Scope: scope, Level: level}
	_, found := l.contexts[key]
	delete(l.contexts, key)
	return found
}

// GetContext returns the cached context for the given scope and level, if any.
func (l *Loader) GetContext(scope, level string) (json.RawMessage, bool) {
	l.mu.Lock()
	defer l.mu.Unlock()
	ctx, ok := l.contexts[contextKey{Scope: scope, Level: level}]
	return ctx, ok
}

// Close clears all cached contexts.
func (l *Loader) Close() {
	l.mu.Lock()
	defer l.mu.Unlock()
	l.contexts = make(map[contextKey]json.RawMessage)
}

func trimTrailingNewlines(b []byte) []byte {
	for len(b) > 0 && (b[len(b)-1] == '\n' || b[len(b)-1] == '\r') {
		b = b[:len(b)-1]
	}
	return b
}
