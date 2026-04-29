package protocol

import (
	"os"
	"path/filepath"
	"testing"
)

// TestAnalyzeResolvesGoWorkCrossModuleImport verifies that the analyzer
// resolves imports across modules joined by a go.work workspace file. The
// test materializes a 2-module workspace under t.TempDir():
//
//	root/
//	  go.work                    (use ./app, ./lib)
//	  app/
//	    go.mod                   (module example.com/app, requires example.com/lib v0.0.0)
//	    app.go                   (imports example.com/lib, returns lib.Gadget)
//	  lib/
//	    go.mod                   (module example.com/lib)
//	    lib.go                   (defines Gadget struct)
//
// The analyzer should resolve lib.Gadget across the workspace boundary and
// report the return type as a struct with the Gadget's fields. With go.work
// resolution broken the typechecker emits a "could not import example.com/lib"
// error and the return type collapses to an opaque kind.
//
// Sister test of vendor_test.go (str-nm5e). Both lock in toolchain-driven
// resolution paths that the analyzer inherits from go/packages without
// analyzer-side flag plumbing.
func TestAnalyzeResolvesGoWorkCrossModuleImport(t *testing.T) {
	// Force network/module-cache resolution off so the only way example.com/lib
	// can be resolved is through the go.work workspace member. If go.work is
	// not honored, the typechecker cannot import the package and the return
	// type collapses.
	withEnv(t, "GOPROXY", "off")
	withEnv(t, "GOFLAGS", "")
	// Ensure we don't inherit a stale GOWORK pointing somewhere else; let the
	// toolchain auto-detect go.work via cwd ancestry.
	withEnv(t, "GOWORK", "")

	root := t.TempDir()
	appDir := filepath.Join(root, "app")
	libDir := filepath.Join(root, "lib")
	if err := os.MkdirAll(appDir, 0o755); err != nil {
		t.Fatalf("mkdir app dir: %v", err)
	}
	if err := os.MkdirAll(libDir, 0o755); err != nil {
		t.Fatalf("mkdir lib dir: %v", err)
	}

	goWork := "" +
		"go 1.23\n" +
		"\n" +
		"use (\n" +
		"\t./app\n" +
		"\t./lib\n" +
		")\n"
	if err := os.WriteFile(filepath.Join(root, "go.work"), []byte(goWork), 0o644); err != nil {
		t.Fatalf("write go.work: %v", err)
	}

	libGoMod := "" +
		"module example.com/lib\n" +
		"\n" +
		"go 1.23\n"
	if err := os.WriteFile(filepath.Join(libDir, "go.mod"), []byte(libGoMod), 0o644); err != nil {
		t.Fatalf("write lib/go.mod: %v", err)
	}

	libGo := "" +
		"package lib\n" +
		"\n" +
		"// Gadget is a workspace-sibling type used to verify go.work resolution.\n" +
		"type Gadget struct {\n" +
		"\tID    int\n" +
		"\tLabel string\n" +
		"}\n"
	if err := os.WriteFile(filepath.Join(libDir, "lib.go"), []byte(libGo), 0o644); err != nil {
		t.Fatalf("write lib/lib.go: %v", err)
	}

	appGoMod := "" +
		"module example.com/app\n" +
		"\n" +
		"go 1.23\n" +
		"\n" +
		"require example.com/lib v0.0.0\n"
	if err := os.WriteFile(filepath.Join(appDir, "go.mod"), []byte(appGoMod), 0o644); err != nil {
		t.Fatalf("write app/go.mod: %v", err)
	}

	appGo := "" +
		"package app\n" +
		"\n" +
		"import \"example.com/lib\"\n" +
		"\n" +
		"// MakeGadget constructs a Gadget from the workspace-sibling lib package.\n" +
		"func MakeGadget(id int, label string) lib.Gadget {\n" +
		"\treturn lib.Gadget{ID: id, Label: label}\n" +
		"}\n"
	appPath := filepath.Join(appDir, "app.go")
	if err := os.WriteFile(appPath, []byte(appGo), 0o644); err != nil {
		t.Fatalf("write app/app.go: %v", err)
	}

	results, err := AnalyzeFile(appPath, "MakeGadget")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	fn := results[0]

	// The return type should resolve to the workspace-sibling Gadget struct
	// (Kind "object" with the two declared fields). If go.work resolution
	// failed the kind would not be "object" and Fields would be empty.
	if fn.ReturnType.Kind != "object" {
		t.Fatalf("return type kind = %q, want \"object\" (go.work lookup likely failed)", fn.ReturnType.Kind)
	}
	if len(fn.ReturnType.Fields) != 2 {
		t.Fatalf("return type fields len = %d, want 2 (go.work lookup did not resolve struct fields)", len(fn.ReturnType.Fields))
	}
	fieldKinds := map[string]string{}
	for _, field := range fn.ReturnType.Fields {
		fieldKinds[field.Name] = field.Type.Kind
	}
	if fieldKinds["ID"] != "int" {
		t.Errorf("Gadget.ID kind = %q, want \"int\"", fieldKinds["ID"])
	}
	if fieldKinds["Label"] != "str" {
		t.Errorf("Gadget.Label kind = %q, want \"str\"", fieldKinds["Label"])
	}
}
