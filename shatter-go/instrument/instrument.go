package instrument

import (
	"bufio"
	"fmt"
	"go/ast"
	"os"
	"path/filepath"
	"strings"

	frontendtiming "github.com/shatter-dev/shatter/shatter-go/timing"
)

// InstrumentFile parses and instruments a Go source file, writing the output
// to a temporary directory. If funcName is non-nil, only that function is
// instrumented. When projectRoot is non-nil, go.mod and go.sum are copied
// from that directory instead of walking up from the source file.
// Returns the output directory path.
func InstrumentFile(sourcePath string, funcName *string, projectRoot *string) (string, error) {
	return InstrumentFileWithTiming(sourcePath, funcName, projectRoot, nil)
}

// InstrumentFileWithTiming instruments a Go source file and records stable phase timings when requested.
// Creates a new temporary directory for the output. To instrument into an existing directory,
// use InstrumentFileToDir.
func InstrumentFileWithTiming(sourcePath string, funcName *string, projectRoot *string, timing *frontendtiming.Collector) (string, error) {
	outputDir, err := os.MkdirTemp("", "shatter-instrument-*")
	if err != nil {
		return "", fmt.Errorf("creating temp dir: %w", err)
	}
	if err := InstrumentFileToDir(sourcePath, outputDir, funcName, projectRoot, timing); err != nil {
		_ = os.RemoveAll(outputDir)
		return "", err
	}
	return outputDir, nil
}

// InstrumentFileToDir instruments a Go source file, writing the output into outputDir.
// The caller is responsible for creating outputDir and cleaning it up.
// If funcName is non-nil, only that function is instrumented. When projectRoot is non-nil,
// go.mod and go.sum are copied from that directory instead of walking up from the source file.
func InstrumentFileToDir(sourcePath, outputDir string, funcName *string, projectRoot *string, timing *frontendtiming.Collector) error {
	packageName, source, err := instrumentSource(sourcePath, funcName, true /*renameMain*/, timing)
	if err != nil {
		return err
	}

	sourceName := filepath.Base(sourcePath)
	outPath := filepath.Join(outputDir, sourceName)
	finishWriteSource := timing.Start("instrument.write_source")
	if err := os.WriteFile(outPath, source, 0644); err != nil {
		finishWriteSource()
		return fmt.Errorf("creating output file: %w", err)
	}
	finishWriteSource()

	// Write recorder
	recorderPath := filepath.Join(outputDir, "shatter_recorder.go")
	recorderSource := generateRecorder(packageName)
	finishWriteRecorder := timing.Start("instrument.write_recorder")
	if err := os.WriteFile(recorderPath, []byte(recorderSource), 0644); err != nil {
		finishWriteRecorder()
		return fmt.Errorf("writing recorder: %w", err)
	}
	finishWriteRecorder()

	// Write go.mod (and go.sum if present)
	finishWriteGoMod := timing.Start("instrument.write_go_mod")
	if err := writeGoMod(outputDir, sourcePath, projectRoot); err != nil {
		finishWriteGoMod()
		return fmt.Errorf("writing go.mod: %w", err)
	}
	finishWriteGoMod()

	return nil
}

// writeGoMod copies go.mod and go.sum from the project root (if provided),
// falls back to walking up from the source directory, or creates a minimal go.mod.
// When copying from a real project, a replace directive is appended so that
// intra-module imports resolve against the original source tree.
func writeGoMod(outputDir, sourcePath string, projectRoot *string) error {
	// Try project root first when provided
	if projectRoot != nil {
		if err := copyModFiles(outputDir, *projectRoot); err == nil {
			return nil
		}
	}

	// Walk up from source directory to find go.mod
	dir := filepath.Dir(sourcePath)
	for {
		if err := copyModFiles(outputDir, dir); err == nil {
			return nil
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
		dir = parent
	}

	// Create a minimal go.mod
	modContent := "module shatter_instrumented\n\ngo 1.23\n"
	return os.WriteFile(filepath.Join(outputDir, "go.mod"), []byte(modContent), 0644)
}

// copyModFiles copies go.mod and go.sum (if present) from srcDir to outputDir.
// After copying, it appends a replace directive that maps the module back to
// srcDir so intra-module imports resolve against the original project tree.
// Returns an error if go.mod does not exist in srcDir.
func copyModFiles(outputDir, srcDir string) error {
	modPath := filepath.Join(srcDir, "go.mod")
	modData, err := os.ReadFile(modPath)
	if err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(outputDir, "go.mod"), modData, 0644); err != nil {
		return err
	}
	// Copy go.sum if it exists (best-effort)
	if sumData, err := os.ReadFile(filepath.Join(srcDir, "go.sum")); err == nil {
		_ = os.WriteFile(filepath.Join(outputDir, "go.sum"), sumData, 0644)
	}

	// Rewrite the module declaration to a harness-local name and add a
	// require+replace for the original module so intra-module imports resolve
	// against the original project source tree. Without this, the temporary
	// harness directory cannot find sub-packages from the same module.
	modulePath := parseModulePath(modData)
	if modulePath != "" {
		absSrcDir, err := filepath.Abs(srcDir)
		if err != nil {
			return fmt.Errorf("resolving module root: %w", err)
		}
		if err := rewriteModuleDecl(outputDir); err != nil {
			return err
		}
		return appendModuleReplace(outputDir, modulePath, absSrcDir)
	}
	return nil
}

// parseModulePath extracts the module path from go.mod content.
// Returns empty string if the module directive is not found.
func parseModulePath(modData []byte) string {
	scanner := bufio.NewScanner(strings.NewReader(string(modData)))
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if strings.HasPrefix(line, "module ") {
			return strings.TrimSpace(strings.TrimPrefix(line, "module "))
		}
	}
	return ""
}

// harnessModuleName is the Go module path used by the instrumented harness.
// The original module declaration is rewritten to this name so that
// intra-module imports are treated as external and resolved via replace.
const harnessModuleName = "shatter_instrumented"

// rewriteModuleDecl replaces the module declaration in go.mod with
// harnessModuleName. This ensures intra-module imports from the original
// project are treated as external dependencies, resolved via replace.
func rewriteModuleDecl(outputDir string) error {
	modPath := filepath.Join(outputDir, "go.mod")
	data, err := os.ReadFile(modPath)
	if err != nil {
		return fmt.Errorf("reading go.mod for rewrite: %w", err)
	}

	var result []string
	scanner := bufio.NewScanner(strings.NewReader(string(data)))
	for scanner.Scan() {
		line := scanner.Text()
		if strings.HasPrefix(strings.TrimSpace(line), "module ") {
			result = append(result, "module "+harnessModuleName)
		} else {
			result = append(result, line)
		}
	}

	rewritten := strings.Join(result, "\n") + "\n"
	return os.WriteFile(modPath, []byte(rewritten), 0644)
}

// appendModuleReplace appends a replace directive to the go.mod in outputDir
// that maps modulePath to the original project root directory.
func appendModuleReplace(outputDir, modulePath, projectRoot string) error {
	modPath := filepath.Join(outputDir, "go.mod")
	f, err := os.OpenFile(modPath, os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		return fmt.Errorf("opening go.mod for module replace: %w", err)
	}
	defer f.Close()

	directive := fmt.Sprintf("\nrequire %s v0.0.0\nreplace %s => %s\n", modulePath, modulePath, projectRoot)
	if _, err := f.WriteString(directive); err != nil {
		return fmt.Errorf("writing module replace directive: %w", err)
	}
	return nil
}

// renameMainFunc renames func main() to avoid redeclaration with the harness,
// while preserving imports that the original main may have used.
// Used when instrumenting package main files: the harness main.go provides the
// entry point, so keeping the original func main() would cause a redeclaration
// error at build time.
func renameMainFunc(file *ast.File) {
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if ok && fn.Name.Name == "main" && fn.Recv == nil {
			fn.Name.Name = "_shatter_original_main_"
		}
	}
}
