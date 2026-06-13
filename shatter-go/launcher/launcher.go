// Package launcher generates and compiles per-target launcher binaries.
//
// A launcher binary is a standalone Go command that imports a target package,
// calls its generated ShatterInvoke function in response to JSON requests read
// from stdin, and writes JSON responses to stdout. Each request carries a
// PlanDescriptor alongside the input slice, following the same pattern as
// harness.RunLoop but extended for wrapper-style dispatch.
//
// Binary caching: BuildLauncher writes the compiled binary to
// <BinariesDir>/shatter_launcher_<DiscoveryHash>[.exe]. Subsequent calls
// with the same DiscoveryHash return immediately without rebuilding.
// Use wrapper.DiscoveryHash to derive the hash.
//
// Source placement (str-b7zh): the launcher is synthesized INSIDE the target
// module under <TargetModuleDir>/.shatter-launchers/<hash>-<pid>/main.go.
// `go build` then runs from the target module root, so the launcher and any
// internal/ packages it imports live in the same module. The transient
// source directory is removed after the build (the compiled binary is
// retained in BinariesDir).
package launcher

import (
	"encoding/json"
	"errors"
	"fmt"
	"maps"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
	"golang.org/x/mod/modfile"
)

const (
	launcherBuildLockPollInterval = 50 * time.Millisecond
	launcherBuildLockStaleAfter   = 30 * time.Minute

	// launchersDirName is the directory created inside the target module to
	// hold transient launcher source packages. It is dot-prefixed so the Go
	// tool skips it in `./...` wildcards and so common .gitignore filters
	// already exclude it.
	launchersDirName = ".shatter-launchers"
)

// activeLauncherDirs tracks transient launcher source directories created by
// in-flight BuildLauncher calls (str-bni0). The deferred cleanup inside
// BuildLauncher handles the happy path and ordinary error returns, but a
// signal-induced process exit (SIGTERM/SIGINT from the parent CLI) bypasses
// Go's defers and would otherwise leave a `.shatter-launchers/` artefact in
// the target project tree. The Go frontend's signal handler calls
// SweepActive() to remove these dirs before the process exits.
var (
	activeLauncherDirsMu sync.Mutex
	activeLauncherDirs   = map[string]struct{}{}
)

func registerActiveLauncherDir(dir string) {
	activeLauncherDirsMu.Lock()
	defer activeLauncherDirsMu.Unlock()
	activeLauncherDirs[dir] = struct{}{}
}

func unregisterActiveLauncherDir(dir string) {
	activeLauncherDirsMu.Lock()
	defer activeLauncherDirsMu.Unlock()
	delete(activeLauncherDirs, dir)
}

// SweepActive removes every transient launcher source directory currently
// tracked as in-flight, along with its now-empty enclosing
// `.shatter-launchers/` parent when possible. Best-effort: errors are
// swallowed so the caller (typically a signal handler racing with process
// exit) can continue tearing down.
func SweepActive() {
	activeLauncherDirsMu.Lock()
	dirs := make([]string, 0, len(activeLauncherDirs))
	for d := range activeLauncherDirs {
		dirs = append(dirs, d)
	}
	activeLauncherDirs = map[string]struct{}{}
	activeLauncherDirsMu.Unlock()

	parents := map[string]struct{}{}
	for _, d := range dirs {
		_ = os.RemoveAll(d)
		parent := filepath.Dir(d)
		if filepath.Base(parent) == launchersDirName {
			parents[parent] = struct{}{}
		}
	}
	for parent := range parents {
		entries, err := os.ReadDir(parent)
		if err == nil && len(entries) == 0 {
			_ = os.Remove(parent)
		}
	}
}

// sweepOrphanedLauncherDirs removes pid-suffixed subdirectories under a
// target module's `.shatter-launchers/` whose pid no longer refers to a live
// process. Called at BuildLauncher start so a previous run that died from a
// signal (and skipped its defers) cannot leak its source dir across runs.
// Best-effort; errors are swallowed.
func sweepOrphanedLauncherDirs(launchersParent string) {
	entries, err := os.ReadDir(launchersParent)
	if err != nil {
		return
	}
	myPid := os.Getpid()
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		name := entry.Name()
		dashIdx := strings.LastIndexByte(name, '-')
		if dashIdx < 0 {
			continue
		}
		pidStr := name[dashIdx+1:]
		pid, parseErr := strconv.Atoi(pidStr)
		if parseErr != nil || pid <= 0 || pid == myPid {
			continue
		}
		if processIsAlive(pid) {
			continue
		}
		_ = os.RemoveAll(filepath.Join(launchersParent, name))
	}
	// If the sweep emptied the parent, remove it so a clean tree is left
	// behind for callers inspecting git status (str-bni0).
	remaining, err := os.ReadDir(launchersParent)
	if err == nil && len(remaining) == 0 {
		_ = os.Remove(launchersParent)
	}
}

// processIsAlive reports whether `pid` definitely refers to a live process.
// Unknown platform results are treated as live so cleanup never removes a
// directory that could still be owned by another process.
func processIsAlive(pid int) bool {
	return processStatus(pid) != processDead
}

type processLiveness int

const (
	processAlive processLiveness = iota
	processDead
	processUnknown
)

func processStatus(pid int) processLiveness {
	proc, err := os.FindProcess(pid)
	if err != nil {
		return processDead
	}
	signalErr := proc.Signal(syscall.Signal(0))
	if signalErr == nil || errors.Is(signalErr, os.ErrPermission) {
		return processAlive
	}
	if runtime.GOOS == "windows" {
		return processUnknown
	}
	return processDead
}

// BuildOptions are the inputs required to build a launcher binary.
type BuildOptions struct {
	// TargetModulePath is the Go module path of the target package
	// (e.g. "example.com/targets").
	TargetModulePath string
	// TargetModuleDir is the on-disk root of the target module. Must contain
	// the module's go.mod and be writable so a transient
	// `.shatter-launchers/<hash>-<pid>/` source directory can be created and
	// torn down.
	TargetModuleDir string
	// TargetImportPath is the import path of the specific target package
	// (often equal to TargetModulePath when the target is the root package).
	TargetImportPath string
	// DiscoveryHash is the 16-char hex hash from wrapper.DiscoveryHash.
	// It determines the binary cache key.
	DiscoveryHash string
	// WrapperRealPath is the on-disk path to the generated wrapper file
	// (produced by wrapper.WriteWrapperFile).
	WrapperRealPath string
	// WrapperInTreePath is the path within the target module tree where the
	// wrapper file should appear during build
	// (e.g. <targetPkgDir>/shatter_wrapper_<hash>.go).
	WrapperInTreePath string
	// GeneratedDir is the workspace area for generated launcher-side
	// artifacts (overlay manifests, augmented go.mod for the harness-loop
	// case). Nothing is written into the target module here.
	GeneratedDir string
	// BinariesDir is the workspace area for compiled binaries.
	// The binary is written to <BinariesDir>/shatter_launcher_<DiscoveryHash>.
	BinariesDir string
	// GoEnv overrides the environment for go build. Nil uses os.Environ().
	GoEnv []string
	// OverlayPath is an optional prebuilt overlay manifest. When set, its
	// entries are merged with any overlay entries the launcher itself
	// requires (wrapper overlay, harness-runtime go.mod overlay).
	OverlayPath string
	// MainSource overrides the generated launcher entrypoint when non-empty.
	// Useful for specialized launcher binaries such as adapter-owned handlers.
	MainSource string
	// UseHarnessLoop switches the generated launcher entrypoint from the simple
	// request/response bridge to the richer harness.RunLoop bridge that returns
	// recorder-backed execution data.
	UseHarnessLoop bool
	// HarnessRuntimeDir is the on-disk path of the shared shatter-harness
	// runtime module when UseHarnessLoop is enabled. The target's go.mod is
	// overlaid with a `require shatter-harness v0.0.0 / replace shatter-harness
	// => HarnessRuntimeDir` augmentation for the duration of the build.
	HarnessRuntimeDir string
}

// BuildLauncher compiles the launcher binary for the given target and caches
// it at BinariesDir/shatter_launcher_<DiscoveryHash>. Returns the binary path
// and whether a fresh build was performed (false means cached binary reused).
func BuildLauncher(opts BuildOptions) (binaryPath string, fresh bool, err error) {
	if opts.DiscoveryHash == "" {
		return "", false, fmt.Errorf("launcher: DiscoveryHash must not be empty")
	}
	if opts.TargetModulePath == "" {
		return "", false, fmt.Errorf("launcher: TargetModulePath must not be empty")
	}
	if opts.TargetModuleDir == "" {
		return "", false, fmt.Errorf("launcher: TargetModuleDir must not be empty")
	}
	if opts.TargetImportPath == "" {
		return "", false, fmt.Errorf("launcher: TargetImportPath must not be empty")
	}

	binaryName := "shatter_launcher_" + opts.DiscoveryHash
	if runtime.GOOS == "windows" {
		binaryName += ".exe"
	}
	if err := os.MkdirAll(opts.BinariesDir, 0o755); err != nil {
		return "", false, fmt.Errorf("launcher: create binaries dir: %w", err)
	}
	binaryPath = filepath.Join(opts.BinariesDir, binaryName)
	if _, statErr := os.Stat(binaryPath); statErr == nil {
		return binaryPath, false, nil
	}

	releaseLock, lockAcquired, err := acquireLauncherBuildLock(binaryPath)
	if err != nil {
		return "", false, err
	}
	if !lockAcquired {
		return binaryPath, false, nil
	}
	defer releaseLock()

	if _, statErr := os.Stat(binaryPath); statErr == nil {
		return binaryPath, false, nil
	}

	// Place launcher source INSIDE the target module so Go's internal/
	// visibility rule treats it as same-module code. When the target
	// import path itself contains `internal/` segments, anchor the
	// launcher under the deepest such parent so the launcher's own import
	// path satisfies the parent-of-internal prefix rule. The pid suffix
	// keeps concurrent builds (different processes) from colliding on the
	// same hash; the per-hash file lock above already serialises within a
	// process.
	anchorRel, err := internalAnchorRel(opts.TargetModulePath, opts.TargetImportPath)
	if err != nil {
		return "", false, err
	}
	launcherDirRel := filepath.Join(anchorRel, launchersDirName, fmt.Sprintf("%s-%d", opts.DiscoveryHash, os.Getpid()))
	launcherDir := filepath.Join(opts.TargetModuleDir, launcherDirRel)

	// str-bni0: sweep any pid-suffixed subdirectories left behind by a prior
	// process that exited via a signal (defers don't run on signal-induced
	// exit). This keeps `git status` clean across runs even when a previous
	// scan was Ctrl-C'd.
	sweepOrphanedLauncherDirs(filepath.Dir(launcherDir))

	if err := writeLauncherSourceDir(launcherDir); err != nil {
		return "", false, err
	}
	registerActiveLauncherDir(launcherDir)
	defer func() {
		unregisterActiveLauncherDir(launcherDir)
		cleanupLauncherSourceDir(opts.TargetModuleDir, launcherDir)
	}()

	mainSrc := opts.MainSource
	if mainSrc == "" {
		mainSrc = GenerateLauncherMain(opts.TargetImportPath)
	}
	if opts.UseHarnessLoop && opts.MainSource == "" {
		mainSrc = GenerateHarnessLauncherMain(opts.TargetImportPath)
	}
	if err := os.WriteFile(filepath.Join(launcherDir, "main.go"), []byte(mainSrc), 0o644); err != nil {
		return "", false, fmt.Errorf("launcher: write main.go: %w", err)
	}

	overlayPath, modFilePath, overlayCleanup, err := buildCombinedOverlay(opts)
	if err != nil {
		return "", false, err
	}
	defer overlayCleanup()

	// Build to a same-directory temp path and atomically rename to the
	// final binaryPath on success. Concurrent BuildLauncher callers (in
	// other goroutines or other processes) cache-check via os.Stat on the
	// final binaryPath; without atomic rename, those callers can observe
	// a partially-written file (go build's copy-fallback opens
	// O_CREATE|O_WRONLY|O_TRUNC), return that path, and then trip a
	// `text file busy` startup error when they exec it (str-0cui).
	tempBinaryPath := fmt.Sprintf("%s.tmp-%d-%d", binaryPath, os.Getpid(), time.Now().UnixNano())

	// `-buildvcs=false` disables Go's VCS stamping for the launcher binary.
	// The launcher is a disposable, generated artifact; stamping serves no
	// purpose, and `-buildvcs=auto` (the default) causes builds to fail with
	// `error obtaining VCS status: exit status 128` whenever the build dir
	// or any ancestor contains a `.git` that the toolchain cannot probe
	// (e.g. when shatter is run against a real module checkout from a
	// generated workspace path). See str-qo1.15.
	buildArgs := []string{"build", "-mod=mod", "-buildvcs=false"}
	if modFilePath != "" {
		buildArgs = append(buildArgs, "-modfile", modFilePath)
	}
	if overlayPath != "" {
		buildArgs = append(buildArgs, "-overlay", overlayPath)
	}
	buildArgs = append(buildArgs, "-o", tempBinaryPath, "./"+filepath.ToSlash(launcherDirRel))

	goEnv := opts.GoEnv
	if goEnv == nil {
		goEnv = os.Environ()
	}
	cmd := exec.Command("go", buildArgs...) //nolint:gosec
	cmd.Dir = opts.TargetModuleDir
	cmd.Env = goEnv
	if out, buildErr := cmd.CombinedOutput(); buildErr != nil {
		_ = os.Remove(tempBinaryPath)
		return "", false, fmt.Errorf("launcher: go build: %w\n%s", buildErr, out)
	}

	if err := os.Rename(tempBinaryPath, binaryPath); err != nil {
		_ = os.Remove(tempBinaryPath)
		return "", false, fmt.Errorf("launcher: publish binary %q: %w", binaryPath, err)
	}

	return binaryPath, true, nil
}

// internalAnchorRel returns the path (relative to the target module root) at
// which the launcher source directory should live. When the target import
// path contains `internal/` segments, the anchor is the parent of the
// deepest `internal/`; otherwise the anchor is the module root (empty
// relative path). The launcher's own in-module import path is then
// `<targetModulePath>/<anchorRel>/.shatter-launchers/<hash>-<pid>`, which
// has the prefix Go's internal-package rule requires for an importer of
// any internal/ package whose parent is at or above the anchor.
func internalAnchorRel(targetModulePath, targetImportPath string) (string, error) {
	if targetImportPath != targetModulePath && !strings.HasPrefix(targetImportPath, targetModulePath+"/") {
		return "", fmt.Errorf(
			"launcher: target import path %q is outside target module %q",
			targetImportPath, targetModulePath,
		)
	}
	segments := strings.Split(targetImportPath, "/")
	deepest := -1
	for i, seg := range segments {
		if seg == "internal" {
			deepest = i
		}
	}
	if deepest <= 0 {
		return "", nil
	}
	anchorImport := strings.Join(segments[:deepest], "/")
	moduleSegs := strings.Count(targetModulePath, "/") + 1
	if deepest <= moduleSegs {
		// internal/ is at or above the module root within the import path;
		// the anchor coincides with the module root.
		return "", nil
	}
	relSegs := segments[moduleSegs:deepest]
	_ = anchorImport
	return filepath.Join(relSegs...), nil
}

// writeLauncherSourceDir creates the transient launcher source directory.
// If the directory already exists (e.g. a previous process crashed without
// cleanup), it is removed and recreated. The pid-suffixed naming makes
// genuine concurrent-process collisions vanishingly unlikely; observing a
// stale directory therefore reflects a crash, not a live peer.
func writeLauncherSourceDir(launcherDir string) error {
	if _, statErr := os.Stat(launcherDir); statErr == nil {
		if rmErr := os.RemoveAll(launcherDir); rmErr != nil {
			return fmt.Errorf("launcher: remove stale source dir %q: %w", launcherDir, rmErr)
		}
	}
	if err := os.MkdirAll(launcherDir, 0o755); err != nil {
		return fmt.Errorf("launcher: create launcher dir: %w", err)
	}
	return nil
}

// cleanupLauncherSourceDir removes the launcher's transient source directory
// and, if it leaves the enclosing `.shatter-launchers/` directory empty AND
// no other launcher dirs are tracked as active, removes that too. The active-
// dir check prevents a race where one goroutine's cleanup removes the parent
// while another goroutine is about to create a sibling subdirectory or while
// a sandbox copyTree walk is traversing it (str-17np). Errors are swallowed:
// cleanup is best-effort, and the build's success/failure has already been
// determined.
func cleanupLauncherSourceDir(targetModuleDir, launcherDir string) {
	_ = os.RemoveAll(launcherDir)
	parent := filepath.Dir(launcherDir)
	if filepath.Base(parent) == launchersDirName && strings.HasPrefix(parent, targetModuleDir) {
		// Only remove the parent if no other goroutine has an active
		// launcher dir under ANY .shatter-launchers/ directory. This is
		// conservative: it may leave the dir behind when peers are active
		// in a different module, but SweepActive cleans it up at shutdown.
		if hasActiveLauncherDirs() {
			return
		}
		entries, err := os.ReadDir(parent)
		if err == nil && len(entries) == 0 {
			_ = os.Remove(parent)
		}
	}
}

// hasActiveLauncherDirs reports whether any launcher source directories are
// currently tracked as in-flight (i.e. other goroutines are building).
func hasActiveLauncherDirs() bool {
	activeLauncherDirsMu.Lock()
	defer activeLauncherDirsMu.Unlock()
	return len(activeLauncherDirs) > 0
}

// buildCombinedOverlay assembles a single overlay manifest combining any
// caller-supplied overlay entries and the wrapper-in-tree override. For
// UseHarnessLoop it also returns an augmented target go.mod path to pass via
// -modfile, which lets the Go tool update generated module metadata without
// modifying the target module on disk. Returns empty paths when not needed.
func buildCombinedOverlay(opts BuildOptions) (overlayPath string, modFilePath string, cleanup func(), err error) {
	noop := func() {}
	replace := map[string]string{}

	if opts.OverlayPath != "" {
		data, readErr := os.ReadFile(opts.OverlayPath)
		if readErr != nil {
			return "", "", noop, fmt.Errorf("launcher: read overlay %q: %w", opts.OverlayPath, readErr)
		}
		var existing map[string]map[string]string
		if jsonErr := json.Unmarshal(data, &existing); jsonErr != nil {
			return "", "", noop, fmt.Errorf("launcher: parse overlay %q: %w", opts.OverlayPath, jsonErr)
		}
		maps.Copy(replace, existing["Replace"])
	}

	if opts.OverlayPath == "" && opts.WrapperRealPath != "" && opts.WrapperInTreePath != "" {
		replace[opts.WrapperInTreePath] = opts.WrapperRealPath
	}

	tempPaths := []string{}
	addTemp := func(p string) { tempPaths = append(tempPaths, p) }
	cleanup = func() {
		for _, p := range tempPaths {
			_ = os.Remove(p)
			if strings.HasSuffix(p, ".mod") {
				_ = os.Remove(strings.TrimSuffix(p, ".mod") + ".sum")
			}
		}
	}

	if opts.UseHarnessLoop {
		augmentedGoMod, augErr := writeAugmentedGoMod(opts)
		if augErr != nil {
			return "", "", noop, augErr
		}
		modFilePath = augmentedGoMod
		addTemp(augmentedGoMod)
	}

	if len(replace) == 0 {
		return "", modFilePath, noop, nil
	}

	manifest := map[string]map[string]string{"Replace": replace}
	data, marshalErr := json.MarshalIndent(manifest, "", "  ")
	if marshalErr != nil {
		return "", "", noop, fmt.Errorf("launcher: marshal overlay manifest: %w", marshalErr)
	}
	if err := os.MkdirAll(opts.GeneratedDir, 0o755); err != nil {
		return "", "", noop, fmt.Errorf("launcher: create generated dir: %w", err)
	}
	overlayPath = filepath.Join(opts.GeneratedDir, "overlay-"+opts.DiscoveryHash+".json")
	if err := os.WriteFile(overlayPath, data, 0o644); err != nil {
		return "", "", noop, fmt.Errorf("launcher: write overlay manifest: %w", err)
	}
	addTemp(overlayPath)
	return overlayPath, modFilePath, cleanup, nil
}

// writeAugmentedGoMod parses the target module's go.mod and writes an
// augmented copy (in the workspace, not the target module) that adds
// `require shatter-harness v0.0.0` and `replace shatter-harness =>
// HarnessRuntimeDir`. Returns the path of the augmented file.
func writeAugmentedGoMod(opts BuildOptions) (string, error) {
	targetGoMod := filepath.Join(opts.TargetModuleDir, "go.mod")
	data, err := os.ReadFile(targetGoMod)
	if err != nil {
		return "", fmt.Errorf("launcher: read target go.mod: %w", err)
	}
	f, err := modfile.Parse("go.mod", data, nil)
	if err != nil {
		return "", fmt.Errorf("launcher: parse target go.mod: %w", err)
	}
	if err := f.AddRequire(instrument.HarnessRuntimeModuleName, "v0.0.0"); err != nil {
		return "", fmt.Errorf("launcher: augment go.mod require: %w", err)
	}
	if err := f.AddReplace(instrument.HarnessRuntimeModuleName, "", opts.HarnessRuntimeDir, ""); err != nil {
		return "", fmt.Errorf("launcher: augment go.mod replace: %w", err)
	}

	// Ensure the augmented go directive is at least as high as the harness
	// module's go directive. Without this, `go build -overlay` fails with
	// "go: updates to go.mod needed, but go.mod is part of the overlay"
	// because Go ≥1.22 enforces that a module's go directive is ≥ the
	// highest go directive of its dependencies, and it cannot update an
	// overlaid go.mod.
	if minGoVer, err := harnessGoVersion(opts.HarnessRuntimeDir); err == nil && minGoVer != "" {
		targetVer := ""
		if f.Go != nil {
			targetVer = f.Go.Version
		}
		if goVersionLess(targetVer, minGoVer) {
			if err := f.AddGoStmt(minGoVer); err != nil {
				return "", fmt.Errorf("launcher: bump go directive to %s: %w", minGoVer, err)
			}
		}
	}

	out, err := f.Format()
	if err != nil {
		return "", fmt.Errorf("launcher: format augmented go.mod: %w", err)
	}
	if err := os.MkdirAll(opts.GeneratedDir, 0o755); err != nil {
		return "", fmt.Errorf("launcher: create generated dir: %w", err)
	}
	dest := filepath.Join(opts.GeneratedDir, "go-mod-overlay-"+opts.DiscoveryHash+".mod")
	if err := os.WriteFile(dest, out, 0o644); err != nil {
		return "", fmt.Errorf("launcher: write augmented go.mod: %w", err)
	}
	return dest, nil
}

// harnessGoVersion reads the harness module's go.mod and returns its go
// directive version string (e.g. "1.23"). Returns ("", nil) if the
// directive is absent.
func harnessGoVersion(harnessDir string) (string, error) {
	data, err := os.ReadFile(filepath.Join(harnessDir, "go.mod"))
	if err != nil {
		return "", err
	}
	f, err := modfile.Parse("go.mod", data, nil)
	if err != nil {
		return "", err
	}
	if f.Go != nil {
		return f.Go.Version, nil
	}
	return "", nil
}

// goVersionLess reports whether version a is strictly less than version b.
// Both must be bare Go version strings like "1.21" or "1.23.1".
// Returns true when a is empty (no go directive) and b is not.
func goVersionLess(a, b string) bool {
	if a == "" {
		return b != ""
	}
	if b == "" {
		return false
	}
	aParts := strings.SplitN(a, ".", 3)
	bParts := strings.SplitN(b, ".", 3)
	// Pad to 3 components for uniform comparison.
	for len(aParts) < 3 {
		aParts = append(aParts, "0")
	}
	for len(bParts) < 3 {
		bParts = append(bParts, "0")
	}
	for i := 0; i < 3; i++ {
		ai, _ := strconv.Atoi(aParts[i])
		bi, _ := strconv.Atoi(bParts[i])
		if ai != bi {
			return ai < bi
		}
	}
	return false
}

func acquireLauncherBuildLock(binaryPath string) (release func(), acquired bool, err error) {
	lockPath := binaryPath + ".lock"
	for {
		lockFile, openErr := os.OpenFile(lockPath, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o644)
		if openErr == nil {
			_, _ = fmt.Fprintf(lockFile, "%d\n", os.Getpid())
			if closeErr := lockFile.Close(); closeErr != nil {
				_ = os.Remove(lockPath)
				return nil, false, fmt.Errorf("launcher: close build lock %q: %w", lockPath, closeErr)
			}
			return func() { _ = os.Remove(lockPath) }, true, nil
		}
		if !os.IsExist(openErr) {
			return nil, false, fmt.Errorf("launcher: acquire build lock %q: %w", lockPath, openErr)
		}

		if _, statErr := os.Stat(binaryPath); statErr == nil {
			return nil, false, nil
		}
		if lockIsStale(lockPath) {
			_ = os.Remove(lockPath)
			continue
		}
		time.Sleep(launcherBuildLockPollInterval)
	}
}

func lockIsStale(lockPath string) bool {
	info, err := os.Stat(lockPath)
	if err != nil {
		return false
	}
	data, readErr := os.ReadFile(lockPath)
	if readErr != nil {
		return true
	}
	pid, parseErr := strconv.Atoi(strings.TrimSpace(string(data)))
	if parseErr != nil || pid <= 0 {
		return time.Since(info.ModTime()) > launcherBuildLockStaleAfter
	}
	switch processStatus(pid) {
	case processDead:
		return true
	case processUnknown:
		return time.Since(info.ModTime()) > launcherBuildLockStaleAfter
	default:
		return false
	}
}

// GenerateLauncherMain generates the main.go source for a launcher binary.
//
// targetImportPath is the import path of the target package — the package
// that contains the generated PlanDescriptor type and ShatterInvoke function.
// The generated source uses only stdlib plus the target package.
func GenerateLauncherMain(targetImportPath string) string {
	const bufSize = 4 * 1024 * 1024

	var b strings.Builder
	b.WriteString("// Code generated by Shatter. DO NOT EDIT.\n")
	b.WriteString("package main\n\n")
	b.WriteString("import (\n")
	b.WriteString("\t\"bufio\"\n")
	b.WriteString("\t\"encoding/json\"\n")
	b.WriteString("\t\"fmt\"\n")
	b.WriteString("\t\"os\"\n\n")
	fmt.Fprintf(&b, "\ttarget %q\n", targetImportPath)
	b.WriteString(")\n\n")

	b.WriteString("type launcherRequest struct {\n")
	b.WriteString("\tPlan   target.PlanDescriptor `json:\"plan\"`\n")
	b.WriteString("\tInputs []json.RawMessage      `json:\"inputs\"`\n")
	b.WriteString("}\n\n")

	b.WriteString("type launcherResponse struct {\n")
	b.WriteString("\tReturnValue json.RawMessage `json:\"return_value,omitempty\"`\n")
	b.WriteString("\tError       string          `json:\"error,omitempty\"`\n")
	b.WriteString("}\n\n")

	b.WriteString("func main() {\n")
	fmt.Fprintf(&b, "\tsc := bufio.NewScanner(os.Stdin)\n")
	fmt.Fprintf(&b, "\tsc.Buffer(make([]byte, %d), %d)\n", bufSize, bufSize)
	b.WriteString("\tenc := json.NewEncoder(os.Stdout)\n")
	b.WriteString("\tfor sc.Scan() {\n")
	b.WriteString("\t\tvar req launcherRequest\n")
	b.WriteString("\t\tif err := json.Unmarshal(sc.Bytes(), &req); err != nil {\n")
	b.WriteString("\t\t\t_ = enc.Encode(launcherResponse{Error: fmt.Sprintf(\"bad request: %v\", err)})\n")
	b.WriteString("\t\t\tcontinue\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tresult, err := target.ShatterInvoke(req.Plan, req.Inputs)\n")
	b.WriteString("\t\tif err != nil {\n")
	b.WriteString("\t\t\t_ = enc.Encode(launcherResponse{Error: err.Error()})\n")
	b.WriteString("\t\t\tcontinue\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\trv, _ := json.Marshal(result)\n")
	b.WriteString("\t\t_ = enc.Encode(launcherResponse{ReturnValue: rv})\n")
	b.WriteString("\t}\n")
	b.WriteString("\tos.Exit(1)\n")
	b.WriteString("}\n")

	return b.String()
}

// GenerateHarnessLauncherMain generates a launcher entrypoint that delegates the
// full loop-mode execution contract to the overlaid target package.
func GenerateHarnessLauncherMain(targetImportPath string) string {
	var b strings.Builder
	b.WriteString("// Code generated by Shatter. DO NOT EDIT.\n")
	b.WriteString("package main\n\n")
	b.WriteString("import (\n")
	b.WriteString("\t\"shatter-harness\"\n\n")
	fmt.Fprintf(&b, "\ttarget %q\n", targetImportPath)
	b.WriteString(")\n\n")
	b.WriteString("func main() {\n")
	b.WriteString("\tharness.RunLoop(func(req harness.Request) harness.Response {\n")
	b.WriteString("\t\traw := target.ShatterExecute(req.Plan, req.Inputs, req.Capture)\n")
	b.WriteString("\t\tresp := harness.Response{\n")
	b.WriteString("\t\t\tReturnValue:   raw.ReturnValue,\n")
	b.WriteString("\t\t\tBranchPath:    raw.BranchPath,\n")
	b.WriteString("\t\t\tLinesExecuted: raw.LinesExecuted,\n")
	b.WriteString("\t\t\tScopeEvents:   raw.ScopeEvents,\n")
	b.WriteString("\t\t\tExternalCalls: raw.ExternalCalls,\n")
	b.WriteString("\t\t\tError:         raw.Error,\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tif raw.ThrownError != nil {\n")
	b.WriteString("\t\t\tresp.ThrownError = &harness.Error{\n")
	b.WriteString("\t\t\t\tErrorType:     raw.ThrownError.ErrorType,\n")
	b.WriteString("\t\t\t\tMessage:       raw.ThrownError.Message,\n")
	b.WriteString("\t\t\t\tStack:         raw.ThrownError.Stack,\n")
	b.WriteString("\t\t\t\tErrorCategory: raw.ThrownError.ErrorCategory,\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tif raw.Performance != nil {\n")
	b.WriteString("\t\t\tresp.Performance = &harness.Perf{\n")
	b.WriteString("\t\t\t\tWallTimeMs:         raw.Performance.WallTimeMs,\n")
	b.WriteString("\t\t\t\tCPUTimeUs:          raw.Performance.CPUTimeUs,\n")
	b.WriteString("\t\t\t\tHeapUsedBytes:      raw.Performance.HeapUsedBytes,\n")
	b.WriteString("\t\t\t\tHeapAllocatedBytes: raw.Performance.HeapAllocatedBytes,\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tif len(raw.SideEffects) > 0 {\n")
	b.WriteString("\t\t\tresp.SideEffects = make([]harness.SideEffect, 0, len(raw.SideEffects))\n")
	b.WriteString("\t\t\tfor _, effect := range raw.SideEffects {\n")
	b.WriteString("\t\t\t\tresp.SideEffects = append(resp.SideEffects, harness.SideEffect{\n")
	b.WriteString("\t\t\t\t\tKind:      effect.Kind,\n")
	b.WriteString("\t\t\t\t\tLevel:     effect.Level,\n")
	b.WriteString("\t\t\t\t\tMessage:   effect.Message,\n")
	b.WriteString("\t\t\t\t\tPath:      effect.Path,\n")
	b.WriteString("\t\t\t\t\tContent:   effect.Content,\n")
	b.WriteString("\t\t\t\t\tMethod:    effect.Method,\n")
	b.WriteString("\t\t\t\t\tURL:       effect.URL,\n")
	b.WriteString("\t\t\t\t\tBody:      effect.Body,\n")
	b.WriteString("\t\t\t\t\tName:      effect.Name,\n")
	b.WriteString("\t\t\t\t\tErrorType: effect.ErrorType,\n")
	b.WriteString("\t\t\t\t\tStack:     effect.Stack,\n")
	b.WriteString("\t\t\t\t\tVariable:  effect.Variable,\n")
	b.WriteString("\t\t\t\t\tValue:     effect.Value,\n")
	b.WriteString("\t\t\t\t\tBefore:    effect.Before,\n")
	b.WriteString("\t\t\t\t\tAfter:     effect.After,\n")
	b.WriteString("\t\t\t\t})\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\treturn resp\n")
	b.WriteString("\t})\n")
	b.WriteString("}\n")
	return b.String()
}

// readTargetGoMod parses the target module's go.mod and returns its go version
// string and replace directives. On any read or parse error, both zero values
// are returned and the caller falls back to defaults.
//
// Retained for export_test compatibility; the in-tree launcher no longer
// writes its own go.mod, so the returned data is informational only.
func readTargetGoMod(targetModuleDir string) (goVersion string, replaces []*modfile.Replace) {
	data, err := os.ReadFile(filepath.Join(targetModuleDir, "go.mod"))
	if err != nil {
		return "", nil
	}
	f, err := modfile.Parse("go.mod", data, nil)
	if err != nil {
		return "", nil
	}
	if f.Go != nil {
		goVersion = f.Go.Version
	}
	return goVersion, f.Replace
}
