package instrument

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/printer"
	"go/token"
	"os"
	"path/filepath"
)

// InstrumentFile parses and instruments a Go source file, writing the output
// to a temporary directory. If funcName is non-nil, only that function is
// instrumented. When projectRoot is non-nil, go.mod and go.sum are copied
// from that directory instead of walking up from the source file.
// Returns the output directory path.
func InstrumentFile(sourcePath string, funcName *string, projectRoot *string) (string, error) {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, sourcePath, nil, parser.ParseComments)
	if err != nil {
		return "", fmt.Errorf("parsing %s: %w", sourcePath, err)
	}

	packageName := file.Name.Name
	transformFile(fset, file, funcName)

	// Remove func main() from package main files: the harness main.go provides
	// the entry point, so a pre-existing func main() would cause a redeclaration
	// error at build time.
	if packageName == "main" {
		removeMainFunc(file)
	}

	outputDir, err := os.MkdirTemp("", "shatter-instrument-*")
	if err != nil {
		return "", fmt.Errorf("creating temp dir: %w", err)
	}

	// Write transformed source
	sourceName := filepath.Base(sourcePath)
	outPath := filepath.Join(outputDir, sourceName)
	outFile, err := os.Create(outPath)
	if err != nil {
		return "", fmt.Errorf("creating output file: %w", err)
	}
	defer outFile.Close()

	if err := printer.Fprint(outFile, fset, file); err != nil {
		return "", fmt.Errorf("printing transformed AST: %w", err)
	}

	// Write recorder
	recorderPath := filepath.Join(outputDir, "shatter_recorder.go")
	recorderSource := generateRecorder(packageName)
	if err := os.WriteFile(recorderPath, []byte(recorderSource), 0644); err != nil {
		return "", fmt.Errorf("writing recorder: %w", err)
	}

	// Write go.mod (and go.sum if present)
	if err := writeGoMod(outputDir, sourcePath, projectRoot); err != nil {
		return "", fmt.Errorf("writing go.mod: %w", err)
	}

	return outputDir, nil
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

// removeMainFunc drops any top-level func main() declarations from the AST.
// Used when instrumenting package main files: the harness main.go provides the
// entry point, so keeping the original func main() would cause a redeclaration
// error at build time.
func removeMainFunc(file *ast.File) {
	filtered := file.Decls[:0]
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if ok && fn.Name.Name == "main" && fn.Recv == nil {
			continue
		}
		filtered = append(filtered, decl)
	}
	file.Decls = filtered
}
