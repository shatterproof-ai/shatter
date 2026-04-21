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

	launcherDir := filepath.Join(opts.GeneratedDir, opts.DiscoveryHash, "launcher")
	if err := os.MkdirAll(launcherDir, 0o755); err != nil {
		return "", false, fmt.Errorf("launcher: create launcher dir: %w", err)
	}

	mainSrc := GenerateLauncherMain(opts.TargetImportPath)
	if err := os.WriteFile(filepath.Join(launcherDir, "main.go"), []byte(mainSrc), 0o644); err != nil {
		return "", false, fmt.Errorf("launcher: write main.go: %w", err)
	}

	goMod := buildLauncherGoMod(
		"shatter-launcher-"+opts.DiscoveryHash,
		opts.TargetModulePath,
		opts.TargetModuleDir,
	)
	if err := os.WriteFile(filepath.Join(launcherDir, "go.mod"), []byte(goMod), 0o644); err != nil {
		return "", false, fmt.Errorf("launcher: write go.mod: %w", err)
	}

	buildArgs := []string{"build", "-o", binaryPath}
	if opts.WrapperRealPath != "" && opts.WrapperInTreePath != "" {
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

func buildLauncherGoMod(moduleName, targetModulePath, targetModuleDir string) string {
	var b strings.Builder
	fmt.Fprintf(&b, "module %s\n\ngo 1.23\n\n", moduleName)
	fmt.Fprintf(&b, "require %s v0.0.0\n\n", targetModulePath)
	fmt.Fprintf(&b, "replace %s => %s\n", targetModulePath, targetModuleDir)
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
