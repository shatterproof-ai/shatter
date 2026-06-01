// Package build implements the shatter-go build orchestrator.
//
// Builder coordinates wrapper generation (D3), launcher compilation (D4), and
// binary registration (D6) for a discovered target package. The registry
// ensures that two invocation plans for the same target discovery hash trigger
// exactly one go build invocation; GOCACHE is pinned to the workspace so that
// object files are shared across builds for targets in the same module.
package build

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
	"github.com/shatter-dev/shatter/shatter-go/launcher"
	"github.com/shatter-dev/shatter/shatter-go/workspace"
	"github.com/shatter-dev/shatter/shatter-go/wrapper"
)

const (
	buildGenerationLockPollInterval = 50 * time.Millisecond
	buildGenerationLockStaleAfter   = 30 * time.Minute
)

// BuildRequest describes a target package for which a launcher binary should
// be compiled. All fields except Constructors are required.
type BuildRequest struct {
	// Targets is the list of discovered invocation targets in the package.
	Targets []wrapper.WrapperTarget
	// Constructors is the list of constructor candidates (may be nil).
	Constructors []wrapper.ConstructorCandidate
	// PackageName is the Go package declaration name (e.g. "targets").
	PackageName string
	// TargetModulePath is the module import path (e.g. "example.com/targets").
	TargetModulePath string
	// TargetModuleDir is the on-disk root of the target module (contains go.mod).
	TargetModuleDir string
	// TargetImportPath is the full import path of the specific package being
	// compiled. Often equal to TargetModulePath for root packages.
	TargetImportPath string
	// TargetPackageDir is the on-disk directory of the target package, used to
	// determine the wrapper file's in-tree path.
	TargetPackageDir string
	// InstrumentedSourceFile enables the J2 loop-harness launcher path. When
	// set, the builder overlays recorder-aware instrumented sources for the
	// target package and produces a launcher binary that returns branch data.
	InstrumentedSourceFile string
	// Mocks carries the current execute/prepare mock configuration. The builder
	// uses it when generating loop-harness support files and when deriving a
	// cache key for mock-sensitive launcher binaries.
	Mocks []instrument.MockConfig
}

// BuildResult is returned by Builder.Build on success.
type BuildResult struct {
	// BinaryPath is the absolute path to the compiled launcher binary.
	BinaryPath string
	// Diagnostics contains structured compiler messages (non-empty only when
	// the build succeeds with warnings, which go currently does not emit; the
	// field is reserved for future linter integration).
	Diagnostics []Diagnostic
	// FromCache is true when the binary was found in the registry and no
	// compilation was performed.
	FromCache bool
}

// Builder orchestrates wrapper generation, launcher compilation, and binary
// caching for a workspace. A single Builder should be shared across all build
// requests to maximise cache hits. Builder is safe for concurrent use.
type Builder struct {
	ws       *workspace.Workspace
	registry *BinaryRegistry
	runID    string
	mu       sync.Mutex
}

// NewBuilder creates a Builder backed by the given workspace.
// The binary registry is loaded from <workspace>/binaries/binary_registry.json.
func NewBuilder(ws *workspace.Workspace) *Builder {
	runID := time.Now().UTC().Format("20060102T150405Z")
	return &Builder{
		ws:       ws,
		registry: NewBinaryRegistry(ws.BinariesDir()),
		runID:    runID,
	}
}

// Build compiles a launcher binary for the package described by req. If the
// binary for this discovery hash is already registered (from this run or a
// previous one), it is returned immediately without recompilation.
//
// Build is safe to call concurrently; concurrent calls for the same discovery
// hash serialise through a per-hash lock so that only one go build runs.
func (b *Builder) Build(ctx context.Context, req BuildRequest) (BuildResult, error) {
	if err := validateRequest(req); err != nil {
		return BuildResult{}, err
	}

	hash := cacheKey(req)

	if path, ok := b.registry.Lookup(hash); ok {
		return BuildResult{BinaryPath: path, FromCache: true}, nil
	}

	b.mu.Lock()
	defer b.mu.Unlock()

	releaseBuildLock, err := acquireBuildGenerationLock(b.ws.GeneratedDir(), hash)
	if err != nil {
		return BuildResult{}, err
	}
	defer releaseBuildLock()

	// Re-load and re-check under the cross-builder lock. Another Builder
	// instance may have produced and registered this binary while we waited.
	b.registry = NewBinaryRegistry(b.ws.BinariesDir())
	if path, ok := b.registry.Lookup(hash); ok {
		return BuildResult{BinaryPath: path, FromCache: true}, nil
	}

	generatedDir := filepath.Join(b.ws.GeneratedDir(), hash)
	if err := os.MkdirAll(generatedDir, 0o755); err != nil {
		return BuildResult{}, fmt.Errorf("build: mkdir generated: %w", err)
	}

	// Generate the wrapper file (D3).
	wrapperDir := filepath.Join(generatedDir, "wrapper")
	if err := os.MkdirAll(wrapperDir, 0o755); err != nil {
		return BuildResult{}, fmt.Errorf("build: mkdir wrapper: %w", err)
	}
	wrapperPkgName := normalizedPackageName(req.PackageName)
	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, wrapperPkgName, req.Targets, req.Constructors)
	if err != nil {
		return BuildResult{}, fmt.Errorf("build: generate wrapper: %w", err)
	}
	wrapperInTree := filepath.Join(req.TargetPackageDir, wrapper.WrapperFilename(wrapper.DiscoveryHash(req.Targets, req.Constructors)))

	overlayPath, harnessRuntimeDir, err := b.writeOverlayManifest(req, hash, generatedDir, wrapperPath, wrapperInTree, wrapperPkgName)
	if err != nil {
		return BuildResult{}, err
	}

	// Compile the launcher binary (D4), capturing output for diagnostics.
	binaryPath, diags, buildErr := b.compileLauncher(ctx, req, hash, generatedDir, overlayPath, harnessRuntimeDir)
	if buildErr != nil {
		return BuildResult{Diagnostics: diags}, buildErr
	}

	if err := b.registry.Register(hash, binaryPath); err != nil {
		return BuildResult{}, fmt.Errorf("build: register binary: %w", err)
	}

	return BuildResult{BinaryPath: binaryPath, Diagnostics: diags}, nil
}

func acquireBuildGenerationLock(rootDir, hash string) (release func(), err error) {
	if rootDir == "" {
		return nil, fmt.Errorf("build: generation lock root must not be empty")
	}
	if hash == "" {
		return nil, fmt.Errorf("build: generation lock hash must not be empty")
	}
	if err := os.MkdirAll(rootDir, 0o755); err != nil {
		return nil, fmt.Errorf("build: create generation lock dir: %w", err)
	}

	lockPath := filepath.Join(rootDir, hash+".build.lock")
	for {
		lockFile, openErr := os.OpenFile(lockPath, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o644)
		if openErr == nil {
			_, _ = fmt.Fprintf(lockFile, "%d\n", os.Getpid())
			if closeErr := lockFile.Close(); closeErr != nil {
				_ = os.Remove(lockPath)
				return nil, fmt.Errorf("build: close generation lock %q: %w", lockPath, closeErr)
			}
			return func() { _ = os.Remove(lockPath) }, nil
		}
		if !os.IsExist(openErr) {
			return nil, fmt.Errorf("build: acquire generation lock %q: %w", lockPath, openErr)
		}
		if buildGenerationLockIsStale(lockPath) {
			_ = os.Remove(lockPath)
			continue
		}
		time.Sleep(buildGenerationLockPollInterval)
	}
}

func buildGenerationLockIsStale(lockPath string) bool {
	info, err := os.Stat(lockPath)
	if err != nil {
		return false
	}
	if time.Since(info.ModTime()) <= buildGenerationLockStaleAfter {
		return false
	}
	data, readErr := os.ReadFile(lockPath)
	if readErr != nil {
		return true
	}
	pid, parseErr := strconv.Atoi(strings.TrimSpace(string(data)))
	if parseErr != nil || pid <= 0 || pid == os.Getpid() {
		return true
	}
	proc, findErr := os.FindProcess(pid)
	return findErr != nil || proc.Signal(syscall.Signal(0)) != nil
}

func (b *Builder) compileLauncher(
	_ context.Context,
	req BuildRequest,
	hash, generatedDir, overlayPath, harnessRuntimeDir string,
) (binaryPath string, diags []Diagnostic, err error) {
	logDir := filepath.Join(b.ws.RunsDir(), b.runID)
	if err := os.MkdirAll(logDir, 0o755); err != nil {
		return "", nil, fmt.Errorf("build: mkdir log dir: %w", err)
	}
	logPath := filepath.Join(logDir, "build_"+hash+".log")

	opts := launcher.BuildOptions{
		TargetModulePath:  req.TargetModulePath,
		TargetModuleDir:   req.TargetModuleDir,
		TargetImportPath:  req.TargetImportPath,
		DiscoveryHash:     hash,
		GeneratedDir:      generatedDir,
		BinariesDir:       b.ws.BinariesDir(),
		GoEnv:             b.ws.GoEnv(),
		OverlayPath:       overlayPath,
		UseHarnessLoop:    req.InstrumentedSourceFile != "",
		HarnessRuntimeDir: harnessRuntimeDir,
	}

	binaryPath, fresh, buildErr := launchBuildWithLog(opts, logPath)
	_ = fresh
	if buildErr != nil {
		logData, readErr := os.ReadFile(logPath)
		if readErr == nil && len(logData) > 0 {
			diags = ParseBuildOutput(string(logData))
		}
		if len(diags) == 0 {
			diags = []Diagnostic{{Kind: DiagnosticKindError, Message: buildErr.Error()}}
		}
		return "", diags, fmt.Errorf("build: compilation failed for %s: %w", hash, buildErr)
	}
	return binaryPath, nil, nil
}

func cacheKey(req BuildRequest) string {
	base := wrapper.DiscoveryHash(req.Targets, req.Constructors)
	if req.InstrumentedSourceFile == "" && len(req.Mocks) == 0 {
		return base
	}

	h := sha256.New()
	fmt.Fprint(h, base, "\x00", req.InstrumentedSourceFile, "\x00")
	for _, mock := range req.Mocks {
		fmt.Fprint(h, mock.Symbol, "\x00")
	}
	return base + "-" + hex.EncodeToString(h.Sum(nil))[:8]
}

// launchBuildWithLog builds the launcher and writes go build output to logPath.
func launchBuildWithLog(opts launcher.BuildOptions, logPath string) (binaryPath string, fresh bool, err error) {
	logFile, openErr := os.Create(logPath)
	if openErr != nil {
		return launcher.BuildLauncher(opts)
	}
	defer logFile.Close()

	// Wrap opts so that build output goes to the log file as well.
	// launcher.BuildLauncher writes CombinedOutput into the error; we capture
	// it separately by patching opts to redirect stderr to the file.
	// Since launcher.BuildLauncher uses exec.Command internally and captures
	// output as a string, we re-run the build and tee the output.
	path, isFresh, buildErr := launcher.BuildLauncher(opts)
	if buildErr != nil {
		_, _ = fmt.Fprintf(logFile, "%v\n", buildErr)
	}
	return path, isFresh, buildErr
}

func validateRequest(req BuildRequest) error {
	switch {
	case len(req.Targets) == 0:
		return fmt.Errorf("build: Targets must not be empty")
	case req.PackageName == "":
		return fmt.Errorf("build: PackageName must not be empty")
	case req.TargetModulePath == "":
		return fmt.Errorf("build: TargetModulePath must not be empty")
	case req.TargetModuleDir == "":
		return fmt.Errorf("build: TargetModuleDir must not be empty")
	case req.TargetImportPath == "":
		return fmt.Errorf("build: TargetImportPath must not be empty")
	case req.TargetPackageDir == "":
		return fmt.Errorf("build: TargetPackageDir must not be empty")
	}
	return nil
}
