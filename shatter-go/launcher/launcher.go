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
package launcher

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
)

const (
	launcherBuildLockPollInterval = 50 * time.Millisecond
	launcherBuildLockStaleAfter   = 30 * time.Minute
)

// BuildOptions are the inputs required to build a launcher binary.
type BuildOptions struct {
	// TargetModulePath is the Go module path of the target package
	// (e.g. "example.com/targets"). Used for the replace directive in the
	// launcher's go.mod.
	TargetModulePath string
	// TargetModuleDir is the on-disk root of the target module. Must contain
	// the module's go.mod and be accessible by the go toolchain.
	TargetModuleDir string
	// TargetImportPath is the import path of the specific target package
	// (often equal to TargetModulePath when the target is the root package).
	TargetImportPath string
	// DiscoveryHash is the 16-char hex hash from wrapper.DiscoveryHash.
	// It determines the binary cache key and the launcher module name.
	DiscoveryHash string
	// WrapperRealPath is the on-disk path to the generated wrapper file
	// (produced by wrapper.WriteWrapperFile).
	WrapperRealPath string
	// WrapperInTreePath is the path within the target module tree where the
	// wrapper file should appear during build
	// (e.g. <targetPkgDir>/shatter_wrapper_<hash>.go).
	WrapperInTreePath string
	// GeneratedDir is the workspace area for generated launcher source files
	// and overlay manifests. Subdirectories are created as needed.
	GeneratedDir string
	// BinariesDir is the workspace area for compiled binaries.
	// The binary is written to <BinariesDir>/shatter_launcher_<DiscoveryHash>.
	BinariesDir string
	// GoEnv overrides the environment for go build. Nil uses os.Environ().
	GoEnv []string
	// OverlayPath is an optional prebuilt overlay manifest. When set, it
	// overrides the WrapperRealPath/WrapperInTreePath pair.
	OverlayPath string
	// MainSource overrides the generated launcher entrypoint when non-empty.
	// Useful for specialized launcher binaries such as adapter-owned handlers.
	MainSource string
	// UseHarnessLoop switches the generated launcher entrypoint from the simple
	// request/response bridge to the richer harness.RunLoop bridge that returns
	// recorder-backed execution data.
	UseHarnessLoop bool
	// HarnessRuntimeDir is the replacement target for the shared shatter-harness
	// module when UseHarnessLoop is enabled.
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

	launcherDir := filepath.Join(opts.GeneratedDir, opts.DiscoveryHash, "launcher")
	if err := os.MkdirAll(launcherDir, 0o755); err != nil {
		return "", false, fmt.Errorf("launcher: create launcher dir: %w", err)
	}

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

	goMod := buildLauncherGoMod(
		"shatter-launcher-"+opts.DiscoveryHash,
		opts.TargetModulePath,
		opts.TargetModuleDir,
		opts.UseHarnessLoop,
		opts.HarnessRuntimeDir,
	)
	if err := os.WriteFile(filepath.Join(launcherDir, "go.mod"), []byte(goMod), 0o644); err != nil {
		return "", false, fmt.Errorf("launcher: write go.mod: %w", err)
	}
	if err := seedLauncherGoSum(opts.TargetModuleDir, launcherDir); err != nil {
		return "", false, err
	}

	buildArgs := []string{"build", "-mod=mod", "-o", binaryPath}
	if opts.OverlayPath != "" {
		buildArgs = append(buildArgs, "-overlay", opts.OverlayPath)
	} else if opts.WrapperRealPath != "" && opts.WrapperInTreePath != "" {
		overlayPath, overlayErr := writeLauncherOverlay(launcherDir, opts.WrapperInTreePath, opts.WrapperRealPath)
		if overlayErr != nil {
			return "", false, overlayErr
		}
		buildArgs = append(buildArgs, "-overlay", overlayPath)
	}
	buildArgs = append(buildArgs, ".")

	goEnv := opts.GoEnv
	if goEnv == nil {
		goEnv = os.Environ()
	}
	cmd := exec.Command("go", buildArgs...) //nolint:gosec
	cmd.Dir = launcherDir
	cmd.Env = goEnv
	if out, buildErr := cmd.CombinedOutput(); buildErr != nil {
		return "", false, fmt.Errorf("launcher: go build: %w\n%s", buildErr, out)
	}

	return binaryPath, true, nil
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
	return err == nil && time.Since(info.ModTime()) > launcherBuildLockStaleAfter
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
	b.WriteString("\t\t\t\t\tKind:     effect.Kind,\n")
	b.WriteString("\t\t\t\t\tLevel:    effect.Level,\n")
	b.WriteString("\t\t\t\t\tMessage:  effect.Message,\n")
	b.WriteString("\t\t\t\t\tVariable: effect.Variable,\n")
	b.WriteString("\t\t\t\t\tBefore:   effect.Before,\n")
	b.WriteString("\t\t\t\t\tAfter:    effect.After,\n")
	b.WriteString("\t\t\t\t})\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\treturn resp\n")
	b.WriteString("\t})\n")
	b.WriteString("}\n")
	return b.String()
}

func buildLauncherGoMod(
	moduleName,
	targetModulePath,
	targetModuleDir string,
	useHarnessLoop bool,
	harnessRuntimeDir string,
) string {
	var b strings.Builder
	fmt.Fprintf(&b, "module %s\n\ngo 1.23.0\n\n", moduleName)
	fmt.Fprintf(&b, "require %s v0.0.0\n\n", targetModulePath)
	fmt.Fprintf(&b, "replace %s => %s\n", targetModulePath, targetModuleDir)
	if useHarnessLoop {
		fmt.Fprintf(&b, "\nrequire %s v0.0.0\n", instrument.HarnessRuntimeModuleName)
		fmt.Fprintf(&b, "replace %s => %s\n", instrument.HarnessRuntimeModuleName, harnessRuntimeDir)
	}
	return b.String()
}

func writeLauncherOverlay(launcherDir, inTreePath, realPath string) (string, error) {
	manifest := map[string]map[string]string{
		"Replace": {inTreePath: realPath},
	}
	manifestJSON, err := json.MarshalIndent(manifest, "", "  ")
	if err != nil {
		return "", fmt.Errorf("launcher: marshal overlay manifest: %w", err)
	}
	overlayPath := filepath.Join(launcherDir, "overlay.json")
	if err := os.WriteFile(overlayPath, manifestJSON, 0o644); err != nil {
		return "", fmt.Errorf("launcher: write overlay manifest: %w", err)
	}
	return overlayPath, nil
}

func seedLauncherGoSum(targetModuleDir, launcherDir string) error {
	sourcePath := filepath.Join(targetModuleDir, "go.sum")
	data, err := os.ReadFile(sourcePath)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return fmt.Errorf("launcher: read target go.sum: %w", err)
	}
	if err := os.WriteFile(filepath.Join(launcherDir, "go.sum"), data, 0o644); err != nil {
		return fmt.Errorf("launcher: write go.sum: %w", err)
	}
	return nil
}
