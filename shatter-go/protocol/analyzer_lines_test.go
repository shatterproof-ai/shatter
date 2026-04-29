package protocol

import (
	"bytes"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestAnalyzeFile_LineNumbersStayWithinTargetFile asserts that the
// StartLine/EndLine of every FunctionAnalysis returned by AnalyzeFile fall
// inside the line count of the file whose path was requested, even when the
// package is multi-file and the package-aware loader merges syntax from
// sibling files. Regression for str-fg8e: a function in a 43-line file was
// reported as start_line=148, panicking shatter-core's fingerprint extractor
// because positions from a longer sibling file leaked into the response
// attributed to the short target file.
func TestAnalyzeFile_LineNumbersStayWithinTargetFile(t *testing.T) {
	moduleRoot := t.TempDir()
	if err := os.WriteFile(filepath.Join(moduleRoot, "go.mod"),
		[]byte("module example.com/multifile_lines\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	// "long.go" — many leading lines so its functions sit on high line numbers.
	longLines := []string{"package multifile_lines", ""}
	for i := 0; i < 60; i++ {
		longLines = append(longLines, "// padding line "+strings.Repeat("x", 4))
	}
	longLines = append(longLines,
		"",
		"func LongLeading() int {",
		"\treturn 1",
		"}",
		"",
	)
	longSource := strings.Join(longLines, "\n")
	longFile := filepath.Join(moduleRoot, "long.go")
	if err := os.WriteFile(longFile, []byte(longSource), 0o644); err != nil {
		t.Fatalf("write long.go: %v", err)
	}

	// "short.go" — a small file whose only function lives at a low line
	// number. If sibling positions leak through, ShortFn's StartLine would
	// jump past short.go's EOF.
	shortSource := `package multifile_lines

func ShortFn() int {
	return 2
}
`
	shortFile := filepath.Join(moduleRoot, "short.go")
	if err := os.WriteFile(shortFile, []byte(shortSource), 0o644); err != nil {
		t.Fatalf("write short.go: %v", err)
	}

	checkLines := func(t *testing.T, target string) {
		t.Helper()
		results, err := AnalyzeFile(target, "")
		if err != nil {
			t.Fatalf("AnalyzeFile(%s): %v", filepath.Base(target), err)
		}
		raw, err := os.ReadFile(target)
		if err != nil {
			t.Fatalf("read %s: %v", target, err)
		}
		lineCount := bytes.Count(raw, []byte{'\n'}) + 1
		if len(results) == 0 {
			t.Fatalf("AnalyzeFile(%s) returned 0 functions", filepath.Base(target))
		}
		for _, fa := range results {
			if fa.StartLine < 1 || fa.StartLine > lineCount {
				t.Errorf("function %s: StartLine = %d, file %s has %d lines",
					fa.Name, fa.StartLine, filepath.Base(target), lineCount)
			}
			if fa.EndLine < fa.StartLine || fa.EndLine > lineCount {
				t.Errorf("function %s: EndLine = %d (StartLine=%d), file %s has %d lines",
					fa.Name, fa.EndLine, fa.StartLine, filepath.Base(target), lineCount)
			}
		}
	}

	t.Run("short", func(t *testing.T) { checkLines(t, shortFile) })
	t.Run("long", func(t *testing.T) { checkLines(t, longFile) })
}

// TestDeclarationBelongsToTargetFile_RejectsSiblingPosition exercises the
// cross-file gate added for str-fg8e directly. A synthetic FileSet with two
// distinct files is parsed; a FuncDecl whose token position belongs to the
// sibling file must be rejected when checked against the target's absolute
// path. This locks the contract in place even if go/packages stops merging
// sibling decls into a single file.Decls slice in some future release.
func TestDeclarationBelongsToTargetFile_RejectsSiblingPosition(t *testing.T) {
	moduleRoot := t.TempDir()
	targetSource := "package belongs\n\nfunc Target() int { return 1 }\n"
	siblingSource := "package belongs\n\nfunc Sibling() int { return 2 }\n"
	targetPath := filepath.Join(moduleRoot, "target.go")
	siblingPath := filepath.Join(moduleRoot, "sibling.go")
	if err := os.WriteFile(targetPath, []byte(targetSource), 0o644); err != nil {
		t.Fatalf("write target: %v", err)
	}
	if err := os.WriteFile(siblingPath, []byte(siblingSource), 0o644); err != nil {
		t.Fatalf("write sibling: %v", err)
	}

	fset := token.NewFileSet()
	targetAST, err := parser.ParseFile(fset, targetPath, nil, parser.ParseComments)
	if err != nil {
		t.Fatalf("parse target: %v", err)
	}
	siblingAST, err := parser.ParseFile(fset, siblingPath, nil, parser.ParseComments)
	if err != nil {
		t.Fatalf("parse sibling: %v", err)
	}

	findFunc := func(file *ast.File, name string) *ast.FuncDecl {
		for _, decl := range file.Decls {
			fn, ok := decl.(*ast.FuncDecl)
			if ok && fn.Name.Name == name {
				return fn
			}
		}
		t.Fatalf("function %q not found", name)
		return nil
	}

	targetFn := findFunc(targetAST, "Target")
	siblingFn := findFunc(siblingAST, "Sibling")
	absTarget, err := filepath.Abs(targetPath)
	if err != nil {
		t.Fatalf("abs target: %v", err)
	}

	if !declarationBelongsToTargetFile(fset, targetFn, absTarget) {
		t.Errorf("declarationBelongsToTargetFile rejected the matching declaration")
	}
	if declarationBelongsToTargetFile(fset, siblingFn, absTarget) {
		t.Errorf("declarationBelongsToTargetFile accepted a sibling-file declaration; this is the str-fg8e bug class")
	}
	if strings.Contains(absTarget, "..") {
		t.Errorf("test setup error: absTarget %q is not normalized", absTarget)
	}
}

// TestAnalyzeFile_OnlyReturnsFunctionsFromTargetFile asserts that the
// analyzer never returns FunctionAnalysis records whose declarations live in
// sibling files. Even if positions stay clamped, attributing a sibling
// function to the requested file would let downstream consumers (fingerprint
// registry, source-excerpt UI) read empty bodies.
func TestAnalyzeFile_OnlyReturnsFunctionsFromTargetFile(t *testing.T) {
	moduleRoot := t.TempDir()
	if err := os.WriteFile(filepath.Join(moduleRoot, "go.mod"),
		[]byte("module example.com/onlytarget\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}
	a := `package onlytarget

func InA() int { return 1 }
`
	b := `package onlytarget

func InB() int { return 2 }
`
	aFile := filepath.Join(moduleRoot, "a.go")
	bFile := filepath.Join(moduleRoot, "b.go")
	if err := os.WriteFile(aFile, []byte(a), 0o644); err != nil {
		t.Fatalf("write a.go: %v", err)
	}
	if err := os.WriteFile(bFile, []byte(b), 0o644); err != nil {
		t.Fatalf("write b.go: %v", err)
	}

	results, err := AnalyzeFile(aFile, "")
	if err != nil {
		t.Fatalf("AnalyzeFile(a.go): %v", err)
	}
	if len(results) != 1 {
		names := make([]string, len(results))
		for i, r := range results {
			names[i] = r.Name
		}
		t.Fatalf("AnalyzeFile(a.go) returned %d functions (%v); want only [InA]", len(results), names)
	}
	if results[0].Name != "InA" {
		t.Errorf("function name = %q, want InA", results[0].Name)
	}
}
