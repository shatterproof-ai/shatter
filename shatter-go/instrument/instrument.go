package instrument

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/printer"
	"go/token"
	"os"
	"path/filepath"

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
	fset := token.NewFileSet()
	finishParse := timing.Start("instrument.parse")
	file, err := parser.ParseFile(fset, sourcePath, nil, parser.ParseComments)
	finishParse()
	if err != nil {
		return fmt.Errorf("parsing %s: %w", sourcePath, err)
	}

	packageName := file.Name.Name
	finishTransform := timing.Start("instrument.transform")
	transformFile(fset, file, funcName)
	finishTransform()

	// Rename func main() in package main files: the harness main.go provides
	// the entry point, so a pre-existing func main() would cause a redeclaration
	// error at build time. Renaming (rather than removing) preserves imports
	// that the original main may have used.
	if packageName == "main" {
		renameMainFunc(file)
	}

	// Write transformed source
	sourceName := filepath.Base(sourcePath)
	outPath := filepath.Join(outputDir, sourceName)
	finishWriteSource := timing.Start("instrument.write_source")
	outFile, err := os.Create(outPath)
	if err != nil {
		finishWriteSource()
		return fmt.Errorf("creating output file: %w", err)
	}
	defer outFile.Close()

	if err := printer.Fprint(outFile, fset, file); err != nil {
		finishWriteSource()
		return fmt.Errorf("printing transformed AST: %w", err)
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
// Returns an error if go.mod does not exist in srcDir.
func copyModFiles(outputDir, srcDir string) error {
	modData, err := os.ReadFile(filepath.Join(srcDir, "go.mod"))
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
