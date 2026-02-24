package instrument

import (
	"fmt"
	"go/parser"
	"go/printer"
	"go/token"
	"os"
	"path/filepath"
)

// InstrumentFile parses and instruments a Go source file, writing the output
// to a temporary directory. If funcName is non-nil, only that function is
// instrumented. Returns the output directory path.
func InstrumentFile(sourcePath string, funcName *string) (string, error) {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, sourcePath, nil, parser.ParseComments)
	if err != nil {
		return "", fmt.Errorf("parsing %s: %w", sourcePath, err)
	}

	packageName := file.Name.Name
	transformFile(fset, file, funcName)

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

	// Write go.mod
	if err := writeGoMod(outputDir, sourcePath); err != nil {
		return "", fmt.Errorf("writing go.mod: %w", err)
	}

	return outputDir, nil
}

// writeGoMod copies go.mod from the source directory or creates a minimal one.
func writeGoMod(outputDir, sourcePath string) error {
	sourceDir := filepath.Dir(sourcePath)
	srcGoMod := filepath.Join(sourceDir, "go.mod")
	if data, err := os.ReadFile(srcGoMod); err == nil {
		return os.WriteFile(filepath.Join(outputDir, "go.mod"), data, 0644)
	}

	// Walk up to find go.mod
	dir := sourceDir
	for {
		candidate := filepath.Join(dir, "go.mod")
		if data, err := os.ReadFile(candidate); err == nil {
			return os.WriteFile(filepath.Join(outputDir, "go.mod"), data, 0644)
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
