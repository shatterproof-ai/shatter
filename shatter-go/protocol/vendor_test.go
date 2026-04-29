package protocol

import (
	"os"
	"path/filepath"
	"testing"
)

// withEnv sets environment variable name to value for the duration of the
// test. The previous value (or unset state) is restored when the test ends.
func withEnv(t *testing.T, name, value string) {
	t.Helper()
	previous, hadPrevious := os.LookupEnv(name)
	if err := os.Setenv(name, value); err != nil {
		t.Fatalf("setenv %s=%q: %v", name, value, err)
	}
	t.Cleanup(func() {
		if hadPrevious {
			_ = os.Setenv(name, previous)
		} else {
			_ = os.Unsetenv(name)
		}
	})
}

// TestAnalyzeResolvesVendoredImport verifies that the analyzer resolves
// imports through a sibling vendor/ directory when the target module's
// vendor/modules.txt is present. Without vendor support the import would
// only be resolvable through GOPATH / module cache, which we sidestep by
// pointing GOPROXY=off and GOFLAGS=-mod=vendor isolation.
//
// The test materializes a tiny module tree under t.TempDir():
//
//	mod/
//	  go.mod                 (module example.com/app, requires example.com/lib v0.0.1)
//	  app.go                 (imports example.com/lib, returns lib.Widget)
//	  vendor/
//	    modules.txt
//	    example.com/lib/
//	      lib.go             (defines Widget struct)
//
// The analyzer should resolve lib.Widget through vendor/ and report the
// return type as a struct with the Widget's fields. With vendor resolution
// broken the typechecker emits an "could not import example.com/lib" error
// and the return type collapses to an opaque kind.
func TestAnalyzeResolvesVendoredImport(t *testing.T) {
	// Force network/module-cache resolution off so vendor is the only
	// possible source for example.com/lib. If vendor lookup is broken the
	// typechecker cannot import the package and the return type collapses.
	withEnv(t, "GOPROXY", "off")
	withEnv(t, "GOFLAGS", "")

	root := t.TempDir()
	moduleDir := filepath.Join(root, "mod")
	vendorLibDir := filepath.Join(moduleDir, "vendor", "example.com", "lib")
	if err := os.MkdirAll(vendorLibDir, 0o755); err != nil {
		t.Fatalf("mkdir vendor lib dir: %v", err)
	}

	goMod := "" +
		"module example.com/app\n" +
		"\n" +
		"go 1.23\n" +
		"\n" +
		"require example.com/lib v0.0.1\n"
	if err := os.WriteFile(filepath.Join(moduleDir, "go.mod"), []byte(goMod), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	// vendor/modules.txt must list the vendored module so the go tool
	// recognizes vendor mode (Go >= 1.14 requires modules.txt to enable
	// vendor consistency checks).
	modulesTxt := "" +
		"# example.com/lib v0.0.1\n" +
		"## explicit\n" +
		"example.com/lib\n"
	if err := os.WriteFile(filepath.Join(moduleDir, "vendor", "modules.txt"), []byte(modulesTxt), 0o644); err != nil {
		t.Fatalf("write modules.txt: %v", err)
	}

	libGo := "" +
		"package lib\n" +
		"\n" +
		"// Widget is a vendored type used to verify vendor resolution.\n" +
		"type Widget struct {\n" +
		"\tID   int\n" +
		"\tName string\n" +
		"}\n"
	if err := os.WriteFile(filepath.Join(vendorLibDir, "lib.go"), []byte(libGo), 0o644); err != nil {
		t.Fatalf("write vendored lib.go: %v", err)
	}

	appGo := "" +
		"package app\n" +
		"\n" +
		"import \"example.com/lib\"\n" +
		"\n" +
		"// MakeWidget constructs a Widget from the vendored lib package.\n" +
		"func MakeWidget(id int, name string) lib.Widget {\n" +
		"\treturn lib.Widget{ID: id, Name: name}\n" +
		"}\n"
	appPath := filepath.Join(moduleDir, "app.go")
	if err := os.WriteFile(appPath, []byte(appGo), 0o644); err != nil {
		t.Fatalf("write app.go: %v", err)
	}

	results, err := AnalyzeFile(appPath, "MakeWidget")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) != 1 {
		t.Fatalf("got %d results, want 1", len(results))
	}
	fn := results[0]

	// The return type should resolve to the vendored Widget struct
	// (Kind "object" with the two declared fields). If vendor resolution
	// failed the kind would not be "object" and Fields would be empty.
	if fn.ReturnType.Kind != "object" {
		t.Fatalf("return type kind = %q, want \"object\" (vendor lookup likely failed)", fn.ReturnType.Kind)
	}
	if len(fn.ReturnType.Fields) != 2 {
		t.Fatalf("return type fields len = %d, want 2 (vendor lookup did not resolve struct fields)", len(fn.ReturnType.Fields))
	}
	fieldKinds := map[string]string{}
	for _, field := range fn.ReturnType.Fields {
		fieldKinds[field.Name] = field.Type.Kind
	}
	if fieldKinds["ID"] != "int" {
		t.Errorf("Widget.ID kind = %q, want \"int\"", fieldKinds["ID"])
	}
	if fieldKinds["Name"] != "str" {
		t.Errorf("Widget.Name kind = %q, want \"str\"", fieldKinds["Name"])
	}
}
