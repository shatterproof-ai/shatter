package instrument

import (
	"bytes"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

// TestInstrumentPackageForOverlay_RewriteSyntaxFixture is the str-jeen.34
// acceptance test.
//
// It runs the Go-frontend rewrite/instrumentation path (the same
// `InstrumentPackageForOverlay` entry point exercised by the
// overlay-builder pipeline) against the checked-in
// `examples/go/rewrite-syntax/` fixture, then parses every emitted
// instrumented file with `go/parser`. The acceptance contract is that
// every output must parse cleanly and reference at least one of the
// `__shatter_record_*` symbols — i.e. the rewriter must not produce
// truncated or otherwise unparseable Go for any of the language
// constructs the fixture covers.
//
// The fixture deliberately covers shapes that have historically tripped
// the rewriter against real-world codebases (see
// `docs/validation/2026-04-go-frontend-kapow-rerun.md` and
// `examples/go/rewrite-syntax/README.md`):
//   - generics (single and multi type-parameter, including `comparable`),
//   - pointer- and value-receiver methods on the same type,
//   - named return values, variadic parameters, multi-return signatures,
//   - type switches with init clauses,
//   - range over slices, maps, channels, plus closure-in-loop bodies,
//   - send (`ch <- v`) and receive (`<-ch`) channel operators,
//   - address-of (`&x`) and dereference (`*p`) — the canonical
//     str-gq7c "unary token" cases,
//   - embedded struct + interface fields,
//   - anonymous struct composite literals.
//
// Cross-ref: str-gq7c (unary token rewrite), str-jdz8 (line-range
// truncation), str-jeen.34 (this fixture).
func TestInstrumentPackageForOverlay_RewriteSyntaxFixture(t *testing.T) {
	fixtureDir, err := rewriteSyntaxFixtureDir()
	if err != nil {
		t.Fatalf("locate fixture: %v", err)
	}
	if _, statErr := os.Stat(filepath.Join(fixtureDir, "go.mod")); statErr != nil {
		t.Skipf("rewrite-syntax fixture not present: %v", statErr)
	}

	generatedDir := t.TempDir()
	files, err := InstrumentPackageForOverlay(fixtureDir, "rewritesyntax", generatedDir)
	if err != nil {
		t.Fatalf("InstrumentPackageForOverlay: %v", err)
	}
	if len(files) == 0 {
		t.Fatal("InstrumentPackageForOverlay returned no files")
	}

	fset := token.NewFileSet()
	for _, f := range files {
		data, readErr := os.ReadFile(f.InstrumentedPath)
		if readErr != nil {
			t.Fatalf("read instrumented %q: %v", f.InstrumentedPath, readErr)
		}
		if _, parseErr := parser.ParseFile(fset, f.InstrumentedPath, data, parser.ParseComments); parseErr != nil {
			t.Fatalf("instrumented file %q does not parse as Go: %v\n--- begin output ---\n%s\n--- end output ---",
				f.InstrumentedPath, parseErr, data)
		}
		// At least one shatter recorder symbol must appear; otherwise the
		// rewriter silently dropped instrumentation and we'd ship an
		// empty harness for this fixture.
		if !bytes.Contains(data, []byte("__shatter_record_")) {
			t.Errorf("instrumented file %q lacks any __shatter_record_* call", f.InstrumentedPath)
		}
	}
}

// rewriteSyntaxFixtureDir resolves the absolute path to
// examples/go/rewrite-syntax/ relative to this test file's location so
// the test runs identically from `go test ./...` and from per-package
// invocations.
func rewriteSyntaxFixtureDir() (string, error) {
	_, thisFile, _, ok := runtime.Caller(0)
	if !ok {
		return "", os.ErrNotExist
	}
	repoRoot := filepath.Join(filepath.Dir(thisFile), "..", "..")
	return filepath.Abs(filepath.Join(repoRoot, "examples", "go", "rewrite-syntax"))
}
