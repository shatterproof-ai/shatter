package build

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/printer"
	"go/token"
	"os"
	"path/filepath"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
	"github.com/shatter-dev/shatter/shatter-go/overlay"
	"github.com/shatter-dev/shatter/shatter-go/workspace"
)

const packageMain = "main"

// wireShimMocks returns the subset of mocks that require a generated
// ShatterMock shim: wire mocks whose behavior is driven by ReturnValues.
// Expression-only mocks (str-c8djq call-site substitution) are excluded — a
// shim for them is never called (review fix 8).
func wireShimMocks(mocks []instrument.MockConfig) []instrument.MockConfig {
	var out []instrument.MockConfig
	for _, m := range mocks {
		if strings.TrimSpace(m.Expression) != "" {
			continue
		}
		out = append(out, m)
	}
	return out
}

func normalizedPackageName(name string) string {
	if name == packageMain {
		return "shattertarget"
	}
	return name
}

func (b *Builder) writeOverlayManifest(
	req BuildRequest,
	hash string,
	generatedDir string,
	wrapperPath string,
	wrapperInTree string,
	packageName string,
) (overlayPath string, harnessRuntimeDir string, err error) {
	overlaysDir := filepath.Join(generatedDir, "overlays")
	builder := overlay.NewBuilder(overlaysDir, hash)
	if err := builder.Add(wrapperInTree, wrapperPath); err != nil {
		return "", "", fmt.Errorf("build: overlay wrapper: %w", err)
	}

	if req.InstrumentedSourceFile != "" {
		instrumentedFiles, err := instrument.InstrumentPackageForOverlay(req.TargetPackageDir, hash, generatedDir)
		if err != nil {
			return "", "", fmt.Errorf("build: instrument package: %w", err)
		}
		if packageName != req.PackageName {
			for _, file := range instrumentedFiles {
				if err := rewriteFilePackage(file.InstrumentedPath, packageName); err != nil {
					return "", "", fmt.Errorf("build: rewrite instrumented package %q: %w", file.InstrumentedPath, err)
				}
			}
		}

		// Execute-time mock substitution (str-c8djq): replace call sites of
		// configured mock symbols with their Go-source expressions in the
		// already-instrumented target sources, so `go build -overlay`
		// compiles the substituted bodies. Applies to expression-bearing
		// mocks only (hint_config_v1 `.shatter/config.yaml` `mocks`); wire
		// mocks carrying only return_values are untouched here.
		//
		// Prefer the pre-resolved MockSubstitutions (type-checked against the
		// loaded package so only genuine package-qualified call sites match);
		// fall back to syntactic substitutions derived from Mocks for callers
		// (e.g. tests) that don't pre-resolve.
		//
		// INVARIANT: len(subs) == 0 here means "the caller never resolved",
		// NOT "resolution proved nothing matches" — resolveMockSubstitutionScopes
		// deliberately returns resolved entries with empty allow-lists rather
		// than filtering them out. Do not "optimize" that away upstream: a
		// filtered-empty resolved set would fall back to syntactic matching
		// for exactly the symbols type resolution proved must not be
		// rewritten.
		subs := req.MockSubstitutions
		if len(subs) == 0 {
			subs = instrument.MockSubstitutionsFromConfigs(req.Mocks)
		}
		if len(subs) > 0 {
			for _, file := range instrumentedFiles {
				if _, err := instrument.RewriteMockCallSitesInFile(file.InstrumentedPath, subs); err != nil {
					return "", "", fmt.Errorf("build: mock substitution %q: %w", file.InstrumentedPath, err)
				}
			}
		}

		if err := instrument.RegisterInstrumentedOverlay(builder, instrumentedFiles); err != nil {
			return "", "", fmt.Errorf("build: register instrumented overlay: %w", err)
		}

		// When the package was renamed (e.g. main → shattertarget),
		// `_test.go` siblings still declare the original package name.
		// `go build` excludes them from the build set, but Go's directory
		// loader still scans every `*.go` file for package consistency
		// and rejects the build with
		//   "found packages shattertarget (X.go) and main (Y_test.go)".
		// Stage rewritten stubs for those siblings so the directory has
		// a single primary package name (and a single _test external).
		// See str-x0sv.
		if packageName != req.PackageName {
			testStubs, err := stageRenamedTestSiblings(req.TargetPackageDir, hash, generatedDir, req.PackageName, packageName)
			if err != nil {
				return "", "", fmt.Errorf("build: stage test siblings: %w", err)
			}
			for _, stub := range testStubs {
				if err := builder.Add(stub.OriginalPath, stub.OverlayPath); err != nil {
					return "", "", fmt.Errorf("build: overlay test sibling %q: %w", stub.OriginalPath, err)
				}
			}
		}

		recorderPath := filepath.Join(generatedDir, "runtime-support", "shatter_recorder_"+hash+".go")
		if err := writeGeneratedSource(recorderPath, instrument.GenerateRecorder(packageName)); err != nil {
			return "", "", fmt.Errorf("build: write recorder: %w", err)
		}
		if err := builder.Add(filepath.Join(req.TargetPackageDir, "shatter_recorder_"+hash+".go"), recorderPath); err != nil {
			return "", "", fmt.Errorf("build: overlay recorder: %w", err)
		}

		globalVars, err := exportedGlobalVars(req.InstrumentedSourceFile)
		if err != nil {
			return "", "", fmt.Errorf("build: analyze globals: %w", err)
		}
		// Only wire mocks (ReturnValues-backed, empty Expression) need the
		// generated ShatterMock shim file; expression-only mocks (str-c8djq)
		// are substituted directly at the call site, so emitting shims for them
		// is dead code in every harness (review fix 8).
		shimMocks := wireShimMocks(req.Mocks)
		runtimePath := filepath.Join(generatedDir, "runtime-support", "shatter_runtime_"+hash+".go")
		if err := writeGeneratedSource(runtimePath, generateRuntimeHelper(packageName, globalVars, len(shimMocks) > 0)); err != nil {
			return "", "", fmt.Errorf("build: write runtime helper: %w", err)
		}
		if err := builder.Add(filepath.Join(req.TargetPackageDir, "shatter_runtime_"+hash+".go"), runtimePath); err != nil {
			return "", "", fmt.Errorf("build: overlay runtime helper: %w", err)
		}

		if len(shimMocks) > 0 {
			mockSource := instrument.GenerateLoopMockFile(shimMocks)
			mockSource = strings.Replace(mockSource, "package main", "package "+packageName, 1)
			mockPath := filepath.Join(generatedDir, "runtime-support", "shatter_mocks_"+hash+".go")
			if err := writeGeneratedSource(mockPath, mockSource); err != nil {
				return "", "", fmt.Errorf("build: write mock support: %w", err)
			}
			if err := builder.Add(filepath.Join(req.TargetPackageDir, "shatter_mocks_"+hash+".go"), mockPath); err != nil {
				return "", "", fmt.Errorf("build: overlay mock support: %w", err)
			}
		}

		harnessRuntimeDir, err = instrument.EnsureHarnessRuntimeDir()
		if err != nil {
			return "", "", fmt.Errorf("build: harness runtime: %w", err)
		}
	}

	overlayPath, err = builder.Write()
	if err != nil {
		return "", "", fmt.Errorf("build: write overlay manifest: %w", err)
	}
	return overlayPath, harnessRuntimeDir, nil
}

func writeGeneratedSource(path, source string) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	if err := os.WriteFile(path, []byte(source), 0o644); err != nil {
		return err
	}
	// Preflight: detect zero-byte materializations before `go build` sees
	// them (str-jeen.51).
	return workspace.VerifyMaterializedSource(path, len(source) > 0)
}

// renamedTestSibling pairs an original `_test.go` path with its rewritten
// overlay stub. The stub keeps the file as a `_test.go` (so it stays
// excluded from `go build`) but rewrites only the package declaration so
// the directory's package set is consistent with the renamed primary
// source.
type renamedTestSibling struct {
	OriginalPath string
	OverlayPath  string
}

// stageRenamedTestSiblings writes a stub for every `_test.go` file in
// packageDir whose package declaration matches the original package name
// (or its `_test` external variant). The stub preserves the external-test
// distinction by rewriting:
//   - `package <orig>`       → `package <new>`
//   - `package <orig>_test`  → `package <new>_test`
//
// File bodies are not preserved: `_test.go` files are excluded from
// `go build` so only the package declaration matters for build-time
// directory consistency. Files declaring an unrelated package name are
// skipped (the directory loader already accepts them).
func stageRenamedTestSiblings(
	packageDir, hash, generatedDir, originalPackage, renamedPackage string,
) ([]renamedTestSibling, error) {
	if packageDir == "" {
		return nil, nil
	}
	matches, err := filepath.Glob(filepath.Join(packageDir, "*_test.go"))
	if err != nil {
		return nil, fmt.Errorf("glob test siblings in %q: %w", packageDir, err)
	}
	if len(matches) == 0 {
		return nil, nil
	}

	stubsDir := filepath.Join(generatedDir, "test-overlay-stubs", hash)
	if err := os.MkdirAll(stubsDir, 0o755); err != nil {
		return nil, fmt.Errorf("mkdir %q: %w", stubsDir, err)
	}

	var staged []renamedTestSibling
	for _, originalPath := range matches {
		fset := token.NewFileSet()
		// Parse only the package clause; we don't need the rest.
		file, parseErr := parser.ParseFile(fset, originalPath, nil, parser.PackageClauseOnly)
		if parseErr != nil {
			return nil, fmt.Errorf("parse %q: %w", originalPath, parseErr)
		}
		filePackage := file.Name.Name

		var stubPackage string
		switch {
		case filePackage == originalPackage:
			stubPackage = renamedPackage
		case filePackage == originalPackage+"_test":
			stubPackage = renamedPackage + "_test"
		default:
			// Some other package (e.g. an unrelated tag-gated file). Leave it alone.
			continue
		}

		stubPath := filepath.Join(stubsDir, filepath.Base(originalPath))
		stubSource := "package " + stubPackage + "\n"
		if err := os.WriteFile(stubPath, []byte(stubSource), 0o644); err != nil {
			return nil, fmt.Errorf("write stub %q: %w", stubPath, err)
		}
		staged = append(staged, renamedTestSibling{
			OriginalPath: originalPath,
			OverlayPath:  stubPath,
		})
	}
	return staged, nil
}

func rewriteFilePackage(path, packageName string) error {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, path, nil, parser.ParseComments)
	if err != nil {
		return err
	}
	file.Name = ast.NewIdent(packageName)
	out, err := os.Create(path)
	if err != nil {
		return err
	}
	if printErr := printer.Fprint(out, fset, file); printErr != nil {
		_ = out.Close()
		return printErr
	}
	if closeErr := out.Close(); closeErr != nil {
		return closeErr
	}
	// Preflight: a zero-byte rewrite would otherwise surface downstream as
	// `expected package, found EOF` from `go build` (str-jeen.51).
	return workspace.VerifyMaterializedSource(path, true)
}

func exportedGlobalVars(sourcePath string) ([]string, error) {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, sourcePath, nil, 0)
	if err != nil {
		return nil, err
	}

	var names []string
	for _, decl := range file.Decls {
		genDecl, ok := decl.(*ast.GenDecl)
		if !ok || genDecl.Tok != token.VAR {
			continue
		}
		for _, spec := range genDecl.Specs {
			valSpec, ok := spec.(*ast.ValueSpec)
			if !ok {
				continue
			}
			for _, name := range valSpec.Names {
				if ast.IsExported(name.Name) {
					names = append(names, name.Name)
				}
			}
		}
	}
	return names, nil
}

func generateRuntimeHelper(packageName string, globalVars []string, hasMocks bool) string {
	var b strings.Builder

	fmt.Fprintf(&b, "package %s\n\n", packageName)
	b.WriteString("import (\n")
	b.WriteString("\t\"bytes\"\n")
	b.WriteString("\t\"encoding/json\"\n")
	b.WriteString("\t\"fmt\"\n\n")
	b.WriteString("\t\"io\"\n")
	b.WriteString("\t\"net/http\"\n")
	b.WriteString("\t\"net/url\"\n")
	b.WriteString("\t\"os\"\n")
	b.WriteString("\t\"runtime\"\n")
	b.WriteString("\t\"strings\"\n")
	b.WriteString("\t\"sync\"\n")
	b.WriteString("\t\"time\"\n")
	b.WriteString(")\n\n")
	b.WriteString("type ShatterSideEffect struct {\n")
	b.WriteString("\tKind      string           `json:\"kind\"`\n")
	b.WriteString("\tLevel     string           `json:\"level,omitempty\"`\n")
	b.WriteString("\tMessage   string           `json:\"message,omitempty\"`\n")
	b.WriteString("\tPath      string           `json:\"path,omitempty\"`\n")
	b.WriteString("\tContent   string           `json:\"content,omitempty\"`\n")
	b.WriteString("\tMethod    string           `json:\"method,omitempty\"`\n")
	b.WriteString("\tURL       string           `json:\"url,omitempty\"`\n")
	b.WriteString("\tBody      *json.RawMessage `json:\"body,omitempty\"`\n")
	b.WriteString("\tName      string           `json:\"name,omitempty\"`\n")
	b.WriteString("\tErrorType string           `json:\"error_type,omitempty\"`\n")
	b.WriteString("\tStack     *string          `json:\"stack,omitempty\"`\n")
	b.WriteString("\tVariable  string           `json:\"variable,omitempty\"`\n")
	b.WriteString("\tValue     *string          `json:\"value,omitempty\"`\n")
	b.WriteString("\tBefore    json.RawMessage  `json:\"before,omitempty\"`\n")
	b.WriteString("\tAfter     json.RawMessage  `json:\"after,omitempty\"`\n")
	b.WriteString("}\n\n")
	b.WriteString("type ShatterError struct {\n")
	b.WriteString("\tErrorType     string `json:\"error_type\"`\n")
	b.WriteString("\tMessage       string `json:\"message\"`\n")
	b.WriteString("\tStack         string `json:\"stack,omitempty\"`\n")
	b.WriteString("\tErrorCategory string `json:\"error_category,omitempty\"`\n")
	b.WriteString("}\n\n")
	b.WriteString("type ShatterPerf struct {\n")
	b.WriteString("\tWallTimeMs         float64 `json:\"wall_time_ms\"`\n")
	b.WriteString("\tCPUTimeUs          int64   `json:\"cpu_time_us\"`\n")
	b.WriteString("\tHeapUsedBytes      int64   `json:\"heap_used_bytes\"`\n")
	b.WriteString("\tHeapAllocatedBytes int64   `json:\"heap_allocated_bytes\"`\n")
	b.WriteString("}\n\n")
	b.WriteString("type ShatterResponse struct {\n")
	b.WriteString("\tReturnValue   json.RawMessage    `json:\"return_value,omitempty\"`\n")
	b.WriteString("\tBranchPath    json.RawMessage    `json:\"branch_path\"`\n")
	b.WriteString("\tLinesExecuted json.RawMessage    `json:\"lines_executed\"`\n")
	b.WriteString("\tScopeEvents   json.RawMessage    `json:\"scope_events\"`\n")
	b.WriteString("\tSideEffects   []ShatterSideEffect `json:\"side_effects\"`\n")
	b.WriteString("\tExternalCalls json.RawMessage    `json:\"external_calls,omitempty\"`\n")
	b.WriteString("\tThrownError   *ShatterError      `json:\"thrown_error,omitempty\"`\n")
	b.WriteString("\tPerformance   *ShatterPerf       `json:\"performance,omitempty\"`\n")
	b.WriteString("\tError         string             `json:\"error,omitempty\"`\n")
	b.WriteString("}\n\n")
	b.WriteString("type shatterCapture struct {\n")
	b.WriteString("\torigOut *os.File\n")
	b.WriteString("\torigErr *os.File\n")
	b.WriteString("\twOut    *os.File\n")
	b.WriteString("\twErr    *os.File\n")
	b.WriteString("\tcapOut  *bytes.Buffer\n")
	b.WriteString("\tcapErr  *bytes.Buffer\n")
	b.WriteString("\tdonOut  chan struct{}\n")
	b.WriteString("\tdonErr  chan struct{}\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterCaptureConsole() *shatterCapture {\n")
	b.WriteString("\tc := &shatterCapture{}\n")
	b.WriteString("\trOut, wOut, _ := os.Pipe()\n")
	b.WriteString("\tc.origOut = os.Stdout\n")
	b.WriteString("\tos.Stdout = wOut\n")
	b.WriteString("\tc.wOut = wOut\n")
	b.WriteString("\tc.capOut = &bytes.Buffer{}\n")
	b.WriteString("\tc.donOut = make(chan struct{})\n")
	b.WriteString("\tgo func() { _, _ = io.Copy(c.capOut, rOut); close(c.donOut) }()\n")
	b.WriteString("\trErr, wErr, _ := os.Pipe()\n")
	b.WriteString("\tc.origErr = os.Stderr\n")
	b.WriteString("\tos.Stderr = wErr\n")
	b.WriteString("\tc.wErr = wErr\n")
	b.WriteString("\tc.capErr = &bytes.Buffer{}\n")
	b.WriteString("\tc.donErr = make(chan struct{})\n")
	b.WriteString("\tgo func() { _, _ = io.Copy(c.capErr, rErr); close(c.donErr) }()\n")
	b.WriteString("\treturn c\n")
	b.WriteString("}\n\n")
	b.WriteString("func (c *shatterCapture) Stop() (string, string) {\n")
	b.WriteString("\tos.Stdout = c.origOut\n")
	b.WriteString("\t_ = c.wOut.Close()\n")
	b.WriteString("\t<-c.donOut\n")
	b.WriteString("\tos.Stderr = c.origErr\n")
	b.WriteString("\t_ = c.wErr.Close()\n")
	b.WriteString("\t<-c.donErr\n")
	b.WriteString("\treturn c.capOut.String(), c.capErr.String()\n")
	b.WriteString("}\n\n")
	b.WriteString("type shatterPerfSnap struct {\n")
	b.WriteString("\tmemBefore runtime.MemStats\n")
	b.WriteString("\tstart     time.Time\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterStartPerf() *shatterPerfSnap {\n")
	b.WriteString("\ts := &shatterPerfSnap{start: time.Now()}\n")
	b.WriteString("\truntime.ReadMemStats(&s.memBefore)\n")
	b.WriteString("\treturn s\n")
	b.WriteString("}\n\n")
	b.WriteString("func (s *shatterPerfSnap) Finish() *ShatterPerf {\n")
	b.WriteString("\telapsed := time.Since(s.start)\n")
	b.WriteString("\tvar memAfter runtime.MemStats\n")
	b.WriteString("\truntime.ReadMemStats(&memAfter)\n")
	b.WriteString("\treturn &ShatterPerf{\n")
	b.WriteString("\t\tWallTimeMs:         float64(elapsed.Microseconds()) / 1000.0,\n")
	b.WriteString("\t\tCPUTimeUs:          elapsed.Microseconds(),\n")
	b.WriteString("\t\tHeapUsedBytes:      int64(memAfter.HeapInuse) - int64(s.memBefore.HeapInuse),\n")
	b.WriteString("\t\tHeapAllocatedBytes: int64(memAfter.TotalAlloc) - int64(s.memBefore.TotalAlloc),\n")
	b.WriteString("\t}\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterSafeCall(fn func()) *ShatterError {\n")
	b.WriteString("\tvar caught *ShatterError\n")
	b.WriteString("\tfunc() {\n")
	b.WriteString("\t\tdefer func() {\n")
	b.WriteString("\t\t\tif r := recover(); r != nil {\n")
	b.WriteString("\t\t\t\tstk := make([]byte, 4096)\n")
	b.WriteString("\t\t\t\tn := runtime.Stack(stk, false)\n")
	b.WriteString("\t\t\t\tcaught = &ShatterError{ErrorType: \"panic\", Message: fmt.Sprintf(\"%v\", r), Stack: string(stk[:n]), ErrorCategory: \"runtime\"}\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t}()\n")
	b.WriteString("\t\tfn()\n")
	b.WriteString("\t}()\n")
	b.WriteString("\treturn caught\n")
	b.WriteString("}\n\n")
	b.WriteString("var (\n")
	b.WriteString("\tshatterSideEffectMu sync.Mutex\n")
	b.WriteString("\tshatterSideEffects   []ShatterSideEffect\n")
	b.WriteString(")\n\n")
	// str-1y6q: capture panics from spawned goroutines. The visitor rewrites
	// every `go X(...)` to `go func() { defer __shatter_recover_goroutine(); X(...) }()`
	// so panics no longer escape and crash the harness process between
	// invocations. ShatterExecute waits a short grace period after the target
	// returns, then drains pending panics into the response's thrown_error.
	b.WriteString("var (\n")
	b.WriteString("\tshatterGoroutinePanicMu sync.Mutex\n")
	b.WriteString("\tshatterGoroutinePanics  []ShatterError\n")
	b.WriteString(")\n\n")
	b.WriteString("func __shatter_recover_goroutine() {\n")
	b.WriteString("\tr := recover()\n")
	b.WriteString("\tif r == nil {\n\t\treturn\n\t}\n")
	b.WriteString("\tstk := make([]byte, 4096)\n")
	b.WriteString("\tn := runtime.Stack(stk, false)\n")
	b.WriteString("\tshatterGoroutinePanicMu.Lock()\n")
	b.WriteString("\tshatterGoroutinePanics = append(shatterGoroutinePanics, ShatterError{ErrorType: \"goroutine_panic\", Message: fmt.Sprintf(\"panic in spawned goroutine: %v\", r), Stack: string(stk[:n]), ErrorCategory: \"runtime\"})\n")
	b.WriteString("\tshatterGoroutinePanicMu.Unlock()\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterDrainGoroutinePanics() []ShatterError {\n")
	b.WriteString("\tshatterGoroutinePanicMu.Lock()\n")
	b.WriteString("\tdefer shatterGoroutinePanicMu.Unlock()\n")
	b.WriteString("\tif len(shatterGoroutinePanics) == 0 {\n\t\treturn nil\n\t}\n")
	b.WriteString("\tout := make([]ShatterError, len(shatterGoroutinePanics))\n")
	b.WriteString("\tcopy(out, shatterGoroutinePanics)\n")
	b.WriteString("\tshatterGoroutinePanics = shatterGoroutinePanics[:0]\n")
	b.WriteString("\treturn out\n")
	b.WriteString("}\n\n")
	// shatterGoroutinePanicGrace is the time we wait after the target returns
	// before sampling spawned-goroutine panics. Short enough to keep
	// invocations snappy; long enough to catch panics that race the
	// response write.
	b.WriteString("const shatterGoroutinePanicGrace = 100 * time.Millisecond\n\n")
	b.WriteString("func shatterRecordSideEffect(effect ShatterSideEffect) {\n")
	b.WriteString("\tshatterSideEffectMu.Lock()\n")
	b.WriteString("\tshatterSideEffects = append(shatterSideEffects, effect)\n")
	b.WriteString("\tshatterSideEffectMu.Unlock()\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterDrainSideEffects() []ShatterSideEffect {\n")
	b.WriteString("\tshatterSideEffectMu.Lock()\n")
	b.WriteString("\tdefer shatterSideEffectMu.Unlock()\n")
	b.WriteString("\tif len(shatterSideEffects) == 0 {\n")
	b.WriteString("\t\treturn nil\n")
	b.WriteString("\t}\n")
	b.WriteString("\tout := make([]ShatterSideEffect, len(shatterSideEffects))\n")
	b.WriteString("\tcopy(out, shatterSideEffects)\n")
	b.WriteString("\tshatterSideEffects = shatterSideEffects[:0]\n")
	b.WriteString("\treturn out\n")
	b.WriteString("}\n\n")
	b.WriteString("var (\n")
	b.WriteString("\tshatterConsoleMu      sync.Mutex\n")
	b.WriteString("\tshatterConsoleEffects []ShatterSideEffect\n")
	b.WriteString(")\n\n")
	b.WriteString("func shatterRecordConsole(level, message string) {\n")
	b.WriteString("\tmessage = strings.TrimRight(message, \"\\r\\n\")\n")
	b.WriteString("\tif message == \"\" {\n")
	b.WriteString("\t\treturn\n")
	b.WriteString("\t}\n")
	b.WriteString("\tshatterConsoleMu.Lock()\n")
	b.WriteString("\tshatterConsoleEffects = append(shatterConsoleEffects, ShatterSideEffect{Kind: \"console_output\", Level: level, Message: message})\n")
	b.WriteString("\tshatterConsoleMu.Unlock()\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterDrainConsoleEffects() []ShatterSideEffect {\n")
	b.WriteString("\tshatterConsoleMu.Lock()\n")
	b.WriteString("\tdefer shatterConsoleMu.Unlock()\n")
	b.WriteString("\tif len(shatterConsoleEffects) == 0 {\n")
	b.WriteString("\t\treturn nil\n")
	b.WriteString("\t}\n")
	b.WriteString("\tout := make([]ShatterSideEffect, len(shatterConsoleEffects))\n")
	b.WriteString("\tcopy(out, shatterConsoleEffects)\n")
	b.WriteString("\tshatterConsoleEffects = shatterConsoleEffects[:0]\n")
	b.WriteString("\treturn out\n")
	b.WriteString("}\n\n")
	b.WriteString("func __shatter_console_fmt_print(level string, fn func(...any) (int, error), args ...any) (int, error) {\n")
	b.WriteString("\tshatterRecordConsole(level, fmt.Sprint(args...))\n")
	b.WriteString("\treturn fn(args...)\n")
	b.WriteString("}\n\n")
	b.WriteString("func __shatter_console_fmt_println(level string, fn func(...any) (int, error), args ...any) (int, error) {\n")
	b.WriteString("\tshatterRecordConsole(level, fmt.Sprintln(args...))\n")
	b.WriteString("\treturn fn(args...)\n")
	b.WriteString("}\n\n")
	b.WriteString("func __shatter_console_fmt_printf(level string, fn func(string, ...any) (int, error), format string, args ...any) (int, error) {\n")
	b.WriteString("\tshatterRecordConsole(level, fmt.Sprintf(format, args...))\n")
	b.WriteString("\treturn fn(format, args...)\n")
	b.WriteString("}\n\n")
	b.WriteString("func __shatter_console_log_print(level string, fn func(...any), args ...any) {\n")
	b.WriteString("\tshatterRecordConsole(level, fmt.Sprint(args...))\n")
	b.WriteString("\tfn(args...)\n")
	b.WriteString("}\n\n")
	b.WriteString("func __shatter_console_log_println(level string, fn func(...any), args ...any) {\n")
	b.WriteString("\tshatterRecordConsole(level, fmt.Sprintln(args...))\n")
	b.WriteString("\tfn(args...)\n")
	b.WriteString("}\n\n")
	b.WriteString("func __shatter_console_log_printf(level string, fn func(string, ...any), format string, args ...any) {\n")
	b.WriteString("\tshatterRecordConsole(level, fmt.Sprintf(format, args...))\n")
	b.WriteString("\tfn(format, args...)\n")
	b.WriteString("}\n\n")
	b.WriteString("func __shatter_console_slog(level string, fn func(string, ...any), msg string, args ...any) {\n")
	b.WriteString("\tshatterRecordConsole(level, msg)\n")
	b.WriteString("\tfn(msg, args...)\n")
	b.WriteString("}\n\n")
	b.WriteString("func __shatter_side_effect_os_write_file(fn func(string, []byte, os.FileMode) error, name string, data []byte, perm os.FileMode) error {\n")
	b.WriteString("\terr := fn(name, data, perm)\n")
	b.WriteString("\tif err == nil {\n")
	b.WriteString("\t\tshatterRecordSideEffect(ShatterSideEffect{Kind: \"file_write\", Path: name, Content: string(data)})\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn err\n")
	b.WriteString("}\n\n")
	b.WriteString("func __shatter_side_effect_os_getenv(fn func(string) string, key string) string {\n")
	b.WriteString("\tvalue := fn(key)\n")
	b.WriteString("\tshatterRecordSideEffect(ShatterSideEffect{Kind: \"environment_read\", Variable: key, Value: &value})\n")
	b.WriteString("\treturn value\n")
	b.WriteString("}\n\n")
	b.WriteString("func __shatter_side_effect_os_lookupenv(fn func(string) (string, bool), key string) (string, bool) {\n")
	b.WriteString("\tvalue, ok := fn(key)\n")
	b.WriteString("\tvar valuePtr *string\n")
	b.WriteString("\tif ok {\n")
	b.WriteString("\t\tvaluePtr = &value\n")
	b.WriteString("\t}\n")
	b.WriteString("\tshatterRecordSideEffect(ShatterSideEffect{Kind: \"environment_read\", Variable: key, Value: valuePtr})\n")
	b.WriteString("\treturn value, ok\n")
	b.WriteString("}\n\n")
	b.WriteString("func __shatter_side_effect_http_get(fn func(string) (*http.Response, error), targetURL string) (*http.Response, error) {\n")
	b.WriteString("\tresp, err := fn(targetURL)\n")
	b.WriteString("\tif err == nil {\n")
	b.WriteString("\t\tshatterRecordSideEffect(ShatterSideEffect{Kind: \"network_request\", Method: \"GET\", URL: targetURL})\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn resp, err\n")
	b.WriteString("}\n\n")
	b.WriteString("func __shatter_side_effect_http_post(fn func(string, string, io.Reader) (*http.Response, error), targetURL, contentType string, body io.Reader) (*http.Response, error) {\n")
	b.WriteString("\tresp, err := fn(targetURL, contentType, body)\n")
	b.WriteString("\tif err == nil {\n")
	b.WriteString("\t\tshatterRecordSideEffect(ShatterSideEffect{Kind: \"network_request\", Method: \"POST\", URL: targetURL})\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn resp, err\n")
	b.WriteString("}\n\n")
	b.WriteString("func __shatter_side_effect_http_post_form(fn func(string, url.Values) (*http.Response, error), targetURL string, data url.Values) (*http.Response, error) {\n")
	b.WriteString("\tresp, err := fn(targetURL, data)\n")
	b.WriteString("\tif err == nil {\n")
	b.WriteString("\t\tshatterRecordSideEffect(ShatterSideEffect{Kind: \"network_request\", Method: \"POST\", URL: targetURL})\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn resp, err\n")
	b.WriteString("}\n\n")
	b.WriteString("func __shatter_side_effect_crypto_rand_read(fn func([]byte) (int, error), buf []byte) (int, error) {\n")
	b.WriteString("\tfor i := range buf {\n")
	b.WriteString("\t\tbuf[i] = byte((i*31 + 17) & 0xff)\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn len(buf), nil\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterConsoleSideEffects(stdout, stderr string) []ShatterSideEffect {\n")
	b.WriteString("\tvar effects []ShatterSideEffect\n")
	b.WriteString("\tif s := strings.TrimSpace(stdout); s != \"\" {\n")
	b.WriteString("\t\teffects = append(effects, ShatterSideEffect{Kind: \"console_output\", Level: \"log\", Message: s})\n")
	b.WriteString("\t}\n")
	b.WriteString("\tif s := strings.TrimSpace(stderr); s != \"\" {\n")
	b.WriteString("\t\teffects = append(effects, ShatterSideEffect{Kind: \"console_output\", Level: \"error\", Message: s})\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn effects\n")
	b.WriteString("}\n\n")
	b.WriteString("func ShatterExecute(planJSON json.RawMessage, inputs []json.RawMessage, capture bool) ShatterResponse {\n")
	b.WriteString("\tvar plan PlanDescriptor\n")
	b.WriteString("\tif err := json.Unmarshal(planJSON, &plan); err != nil {\n")
	b.WriteString("\t\treturn ShatterResponse{Error: fmt.Sprintf(\"unmarshal plan: %v\", err)}\n")
	b.WriteString("\t}\n\n")
	b.WriteString("\t__shatter_reset()\n")
	b.WriteString("\t_ = shatterDrainSideEffects()\n")
	b.WriteString("\t_ = shatterDrainConsoleEffects()\n")
	if hasMocks {
		b.WriteString("\tshatterResetMockCounters()\n")
	}
	b.WriteString("\n")

	for _, name := range globalVars {
		fmt.Fprintf(&b, "\t_bef_%s, _ok_%s := func() (json.RawMessage, bool) {\n", name, name)
		fmt.Fprintf(&b, "\t\t_b, _e := json.Marshal(%s)\n", name)
		b.WriteString("\t\treturn _b, _e == nil\n")
		b.WriteString("\t}()\n")
	}
	if len(globalVars) > 0 {
		b.WriteString("\n")
	}

	b.WriteString("\t_perf := shatterStartPerf()\n")
	b.WriteString("\t_cap := shatterCaptureConsole()\n")
	b.WriteString("\tvar (\n")
	b.WriteString("\t\t_ret       any\n")
	b.WriteString("\t\t_invokeErr error\n")
	b.WriteString("\t)\n")
	b.WriteString("\t_goroutinesBefore := runtime.NumGoroutine()\n")
	b.WriteString("\t_thrownErr := shatterSafeCall(func() {\n")
	b.WriteString("\t\t_ret, _invokeErr = ShatterInvoke(plan, inputs)\n")
	b.WriteString("\t})\n")
	// str-1y6q: if the target spawned goroutines, wait briefly for them to
	// either settle or surface a panic via __shatter_recover_goroutine.
	// Skip the wait entirely when no extra goroutines exist so the common
	// path stays fast.
	b.WriteString("\tif runtime.NumGoroutine() > _goroutinesBefore {\n")
	b.WriteString("\t\t_deadline := time.Now().Add(shatterGoroutinePanicGrace)\n")
	b.WriteString("\t\tfor time.Now().Before(_deadline) {\n")
	b.WriteString("\t\t\tshatterGoroutinePanicMu.Lock()\n")
	b.WriteString("\t\t\t_hasPanic := len(shatterGoroutinePanics) > 0\n")
	b.WriteString("\t\t\tshatterGoroutinePanicMu.Unlock()\n")
	b.WriteString("\t\t\tif _hasPanic || runtime.NumGoroutine() <= _goroutinesBefore {\n")
	b.WriteString("\t\t\t\tbreak\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t\ttime.Sleep(time.Millisecond)\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t}\n")
	b.WriteString("\tif _goPanics := shatterDrainGoroutinePanics(); len(_goPanics) > 0 && _thrownErr == nil {\n")
	b.WriteString("\t\t_first := _goPanics[0]\n")
	b.WriteString("\t\t_thrownErr = &_first\n")
	b.WriteString("\t}\n")
	b.WriteString("\t_stdout, _stderr := _cap.Stop()\n")
	b.WriteString("\t_perfResult := _perf.Finish()\n")
	b.WriteString("\t_rec := __shatter_collect_results()\n")
	b.WriteString("\t_branchPath, _ := json.Marshal(_rec.BranchPath)\n")
	b.WriteString("\t_linesExec, _ := json.Marshal(_rec.LinesExecuted)\n")
	b.WriteString("\t_scopeEvts, _ := json.Marshal(_rec.ScopeEvents)\n\n")

	b.WriteString("\t_resp := ShatterResponse{\n")
	b.WriteString("\t\tBranchPath:    _branchPath,\n")
	b.WriteString("\t\tLinesExecuted: _linesExec,\n")
	b.WriteString("\t\tScopeEvents:   _scopeEvts,\n")
	b.WriteString("\t\tThrownError:   _thrownErr,\n")
	b.WriteString("\t\tPerformance:   _perfResult,\n")
	b.WriteString("\t}\n\n")

	b.WriteString("\tif _invokeErr != nil && _resp.ThrownError == nil {\n")
	b.WriteString("\t\t_resp.ThrownError = &ShatterError{ErrorType: \"function_error\", Message: _invokeErr.Error(), ErrorCategory: \"runtime\"}\n")
	b.WriteString("\t} else if _ret != nil {\n")
	b.WriteString("\t\tif _rv, _e := json.Marshal(_ret); _e == nil {\n")
	b.WriteString("\t\t\t_resp.ReturnValue = _rv\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t}\n\n")

	b.WriteString("\tif capture {\n")
	b.WriteString("\t\tif _sideEffects := shatterDrainSideEffects(); len(_sideEffects) > 0 {\n")
	b.WriteString("\t\t\t_resp.SideEffects = append(_resp.SideEffects, _sideEffects...)\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tif _consoleEffects := shatterDrainConsoleEffects(); len(_consoleEffects) > 0 {\n")
	b.WriteString("\t\t\t_resp.SideEffects = append(_resp.SideEffects, _consoleEffects...)\n")
	b.WriteString("\t\t} else {\n")
	b.WriteString("\t\t\t_resp.SideEffects = append(_resp.SideEffects, shatterConsoleSideEffects(_stdout, _stderr)...)\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tif _resp.ThrownError != nil {\n")
	b.WriteString("\t\t\t_stack := _resp.ThrownError.Stack\n")
	b.WriteString("\t\t\t_resp.SideEffects = append(_resp.SideEffects, ShatterSideEffect{Kind: \"thrown_error\", ErrorType: _resp.ThrownError.ErrorType, Message: _resp.ThrownError.Message, Stack: &_stack})\n")
	b.WriteString("\t\t}\n")
	for _, name := range globalVars {
		fmt.Fprintf(&b, "\t\tif _ok_%s {\n", name)
		fmt.Fprintf(&b, "\t\t\tif _aft_%s, _e := json.Marshal(%s); _e == nil {\n", name, name)
		fmt.Fprintf(&b, "\t\t\t\tif string(_aft_%s) != string(_bef_%s) {\n", name, name)
		b.WriteString("\t\t\t\t\t_resp.SideEffects = append(_resp.SideEffects, ShatterSideEffect{\n")
		b.WriteString("\t\t\t\t\t\tKind:     \"global_state_change\",\n")
		fmt.Fprintf(&b, "\t\t\t\t\t\tVariable: %q,\n", name)
		fmt.Fprintf(&b, "\t\t\t\t\t\tBefore:   _bef_%s,\n", name)
		fmt.Fprintf(&b, "\t\t\t\t\t\tAfter:    _aft_%s,\n", name)
		b.WriteString("\t\t\t\t\t})\n")
		fmt.Fprintf(&b, "\t\t\t\t\t_resp.SideEffects = append(_resp.SideEffects, ShatterSideEffect{Kind: \"global_mutation\", Name: %q})\n", name)
		b.WriteString("\t\t\t\t}\n")
		b.WriteString("\t\t\t}\n")
		b.WriteString("\t\t}\n")
	}
	b.WriteString("\t}\n")
	if hasMocks {
		b.WriteString("\tif _mockCalls := shatterGetAndResetMockCalls(); len(_mockCalls) > 0 {\n")
		b.WriteString("\t\t_resp.ExternalCalls, _ = json.Marshal(_mockCalls)\n")
		b.WriteString("\t}\n")
	}
	b.WriteString("\n\treturn _resp\n")
	b.WriteString("}\n")

	return b.String()
}
