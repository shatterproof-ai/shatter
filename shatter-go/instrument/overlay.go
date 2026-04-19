package instrument

import (
	"bytes"
	"fmt"
	"go/parser"
	"go/printer"
	"go/token"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/overlay"
	frontendtiming "github.com/shatter-dev/shatter/shatter-go/timing"
)

const instrumentedSubdir = "instrumented"

// InstrumentedFile records a single original→instrumented pair produced for
// the overlay manifest by InstrumentPackageForOverlay.
type InstrumentedFile struct {
	OriginalPath     string
	InstrumentedPath string
	PackageName      string
}

// instrumentSource parses sourcePath, applies the existing concolic
// transform, and returns the package name plus the formatted instrumented
// source bytes. If renameMain is true and the package is "main", the
// existing func main() is renamed so a wrapper-provided main() can take
// over (the legacy temp-dir flow needs this; the overlay flow does not).
//
// Extracted from InstrumentFileToDir so both the temp-dir path and the
// overlay path share one parse/transform/print implementation.
func instrumentSource(sourcePath string, funcName *string, renameMain bool, timing *frontendtiming.Collector) (string, []byte, error) {
	fset := token.NewFileSet()
	finishParse := timing.Start("instrument.parse")
	file, err := parser.ParseFile(fset, sourcePath, nil, parser.ParseComments)
	finishParse()
	if err != nil {
		return "", nil, fmt.Errorf("parsing %s: %w", sourcePath, err)
	}

	packageName := file.Name.Name
	finishTransform := timing.Start("instrument.transform")
	transformFile(fset, file, funcName)
	finishTransform()

	if renameMain && packageName == "main" {
		renameMainFunc(file)
	}

	var buf bytes.Buffer
	if err := printer.Fprint(&buf, fset, file); err != nil {
		return "", nil, fmt.Errorf("printing transformed AST for %s: %w", sourcePath, err)
	}
	return packageName, buf.Bytes(), nil
}

// InstrumentPackageForOverlay parses every non-test .go file under
// packageDir, applies the existing concolic AST instrumentation, and
// writes each rewritten source to
// <generatedDir>/<discoveryHash>/instrumented/<basename>.
//
// The recorder symbols referenced by the instrumented code
// (__shatter_record_*, __shatter_reset, …) are NOT emitted here; the
// wrapper produced by D3 owns them. Likewise no go.mod is written: the
// overlay leaves the original module tree intact and `go build -overlay`
// resolves intra-module imports against it.
//
// Files ending in _test.go are excluded. An empty package is an error —
// callers should not invoke this on a directory with no Go sources.
func InstrumentPackageForOverlay(packageDir, discoveryHash, generatedDir string) ([]InstrumentedFile, error) {
	if packageDir == "" {
		return nil, fmt.Errorf("instrument: InstrumentPackageForOverlay: packageDir must not be empty")
	}
	if discoveryHash == "" {
		return nil, fmt.Errorf("instrument: InstrumentPackageForOverlay: discoveryHash must not be empty")
	}
	if generatedDir == "" {
		return nil, fmt.Errorf("instrument: InstrumentPackageForOverlay: generatedDir must not be empty")
	}

	absPackageDir, err := filepath.Abs(packageDir)
	if err != nil {
		return nil, fmt.Errorf("instrument: absolutize packageDir %q: %w", packageDir, err)
	}

	matches, err := filepath.Glob(filepath.Join(absPackageDir, "*.go"))
	if err != nil {
		return nil, fmt.Errorf("instrument: glob %q: %w", absPackageDir, err)
	}

	sources := make([]string, 0, len(matches))
	for _, match := range matches {
		if strings.HasSuffix(match, "_test.go") {
			continue
		}
		sources = append(sources, match)
	}
	sort.Strings(sources)

	if len(sources) == 0 {
		return nil, fmt.Errorf("instrument: no non-test .go files in %q", absPackageDir)
	}

	outDir := filepath.Join(generatedDir, discoveryHash, instrumentedSubdir)
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return nil, fmt.Errorf("instrument: mkdir %q: %w", outDir, err)
	}
	absOutDir, err := filepath.Abs(outDir)
	if err != nil {
		return nil, fmt.Errorf("instrument: absolutize outDir %q: %w", outDir, err)
	}

	results := make([]InstrumentedFile, 0, len(sources))
	for _, sourcePath := range sources {
		packageName, source, err := instrumentSource(sourcePath, nil, false /*renameMain*/, nil)
		if err != nil {
			return nil, err
		}
		if packageName == "" {
			return nil, fmt.Errorf("instrument: %q has empty package name", sourcePath)
		}
		instrumentedPath := filepath.Join(absOutDir, filepath.Base(sourcePath))
		if err := os.WriteFile(instrumentedPath, source, 0o644); err != nil {
			return nil, fmt.Errorf("instrument: write %q: %w", instrumentedPath, err)
		}
		results = append(results, InstrumentedFile{
			OriginalPath:     sourcePath,
			InstrumentedPath: instrumentedPath,
			PackageName:      packageName,
		})
	}
	return results, nil
}

// RegisterInstrumentedOverlay adds an entry to b for every pair so that
// `go build -overlay <manifest>` substitutes each original file with its
// instrumented counterpart at build time.
func RegisterInstrumentedOverlay(b *overlay.Builder, files []InstrumentedFile) error {
	if b == nil {
		return fmt.Errorf("instrument: RegisterInstrumentedOverlay: builder must not be nil")
	}
	for _, f := range files {
		if err := b.Add(f.OriginalPath, f.InstrumentedPath); err != nil {
			return fmt.Errorf("instrument: register overlay for %q: %w", f.OriginalPath, err)
		}
	}
	return nil
}
