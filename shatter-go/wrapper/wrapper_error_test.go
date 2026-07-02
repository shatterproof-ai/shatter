package wrapper_test

import (
	"bytes"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/wrapper"
)

// TestWrapper_ErrorParamRuntimeValueSkipsErrorsImport guards the str-jn9r0
// import-threading predicate. An `error` param satisfied by a runtime-value
// expression is emitted as a direct Go expression (writeParamDeserialization*
// short-circuits on RuntimeValueExpr), so writeErrorParamDeserialization —
// and thus errors.New — is never emitted for it. wrapperNeedsErrorImport must
// therefore NOT thread the "errors" import for such params; otherwise the
// generated wrapper carries an unused import and fails `go build`, killing
// every execute for the module. Covers both the target and constructor legs,
// including the constructor path that resolves through
// constructorParamRuntimeValueExpr.
func TestWrapper_ErrorParamRuntimeValueSkipsErrorsImport(t *testing.T) {
	targets := []wrapper.WrapperTarget{
		{
			ID:         "example.com/rv:TakesErr",
			SymbolName: "TakesErr",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				// Runtime-value-satisfied error param: emitted as `error(nil)`,
				// never through the errors.New decode block.
				{Name: "err", GoType: "error", RuntimeValueExpr: "error(nil)"},
			},
			HasResult:    true,
			ResultGoType: "string",
			ResultCount:  1,
		},
	}
	constructors := []wrapper.ConstructorCandidate{
		{
			FuncName:   "NewThing",
			TargetType: "Thing",
			HasParams:  true,
			Parameters: []wrapper.ConstructorParam{
				{Name: "e", GoType: "error", RuntimeValueExpr: "error(nil)"},
			},
		},
	}

	src := wrapper.GenerateWrapper("rv", targets, constructors)

	if strings.Contains(src, "\t\"errors\"\n") {
		t.Errorf("runtime-value-satisfied error params must not thread the \"errors\" import\nsource:\n%s", src)
	}
	if strings.Contains(src, "errors.New(") {
		t.Errorf("runtime-value-satisfied error param must not emit the errors.New decode block\nsource:\n%s", src)
	}
	// Sanity: the direct runtime-value expression is what got emitted.
	if !strings.Contains(src, "error(nil)") {
		t.Errorf("expected direct runtime-value expression in generated source\nsource:\n%s", src)
	}
}

// TestWrapper_ErrorParam is the str-jn9r0 regression. A bare builtin `error`
// parameter must (a) generate the dedicated decode block that accepts both a
// JSON `null` (nil error) and the cross-frontend
// `{"__complex_type":"error","class":...,"message":m}` shape emitted by the
// Rust core's random generator, (b) compile against the target module, and
// (c) decode each form correctly through ShatterInvoke at run time so both the
// nil and non-nil branches are reachable.
func TestWrapper_ErrorParam(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package errortarget

// Classify mirrors examples/go/error-param/classify.go. The nil / non-nil
// branches give the wrapper-driven invocation observable outcomes per input
// form.
func Classify(err error) string {
	if err == nil {
		return "ok"
	}
	return "err"
}
`
	if err := os.WriteFile(filepath.Join(modDir, "errortarget.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write errortarget.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/errortarget\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	targets := []wrapper.WrapperTarget{
		{
			ID:         "example.com/errortarget:Classify",
			SymbolName: "Classify",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				{Name: "err", GoType: "error"},
			},
			HasResult:    true,
			ResultGoType: "string",
			ResultCount:  1,
		},
	}

	src := wrapper.GenerateWrapper("errortarget", targets, nil)

	// Static guards on the generated source.
	mustContain := []string{
		"\"errors\"",                             // import threaded through
		"var err error",                          // declared as the bare interface
		"_shatterErr",                            // object-fallback struct
		"_shatterErr.ComplexType != \"error\"",   // tag check
		"err = errors.New(*_shatterErr.Message)", // message → error reconstruction
	}
	for _, want := range mustContain {
		if !strings.Contains(src, want) {
			t.Errorf("generated wrapper missing %q\nsource:\n%s", want, src)
		}
	}

	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "errortarget", targets, nil)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}
	hash := wrapper.DiscoveryHash(targets, nil)
	inTreePath := filepath.Join(modDir, wrapper.WrapperFilename(hash))
	manifest := map[string]map[string]string{"Replace": {inTreePath: wrapperPath}}
	manifestJSON, err := json.MarshalIndent(manifest, "", "  ")
	if err != nil {
		t.Fatalf("marshal overlay: %v", err)
	}
	manifestPath := filepath.Join(wrapperDir, "overlay.json")
	if err := os.WriteFile(manifestPath, manifestJSON, 0o644); err != nil {
		t.Fatalf("write overlay: %v", err)
	}

	cmd := exec.Command("go", "build", "-buildvcs=false", "-overlay", manifestPath, "./...")
	cmd.Dir = modDir
	cmd.Env = append(os.Environ(), "GOFLAGS=")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		got, _ := os.ReadFile(wrapperPath)
		t.Fatalf("go build failed: %v\nstderr: %s\ngenerated wrapper:\n%s", err, stderr.String(), got)
	}

	// Drive the generated wrapper through a tiny main.go that calls
	// ShatterInvoke with each input form and prints the result. JSON null must
	// yield the nil branch; the tagged object (with an empty and a non-empty
	// message) must yield the non-nil branch.
	const runnerSrc = `package main

import (
	"encoding/json"
	"fmt"
	"os"

	errortarget "example.com/errortarget"
)

func main() {
	inputs := []json.RawMessage{
		json.RawMessage("null"),                                                       // nil error
		json.RawMessage(` + "`" + `{"__complex_type":"error","class":"Error","message":"boom"}` + "`" + `), // non-nil
		json.RawMessage(` + "`" + `{"__complex_type":"error","class":"TypeError","message":""}` + "`" + `), // non-nil, empty msg
	}
	want := []string{"ok", "err", "err"}
	for i, in := range inputs {
		got, err := errortarget.ShatterInvoke(errortarget.PlanDescriptor{TargetID: "example.com/errortarget:Classify"}, []json.RawMessage{in})
		if err != nil {
			fmt.Fprintf(os.Stderr, "case %d: ShatterInvoke error: %v\n", i, err)
			os.Exit(1)
		}
		g, ok := got.(string)
		if !ok {
			fmt.Fprintf(os.Stderr, "case %d: result type %T, want string\n", i, got)
			os.Exit(1)
		}
		if g != want[i] {
			fmt.Fprintf(os.Stderr, "case %d: got %q, want %q\n", i, g, want[i])
			os.Exit(1)
		}
	}
	fmt.Println("ok")
}
`
	runnerDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(runnerDir, "main.go"), []byte(runnerSrc), 0o644); err != nil {
		t.Fatalf("write main.go: %v", err)
	}
	runnerMod := "module example.com/errorrunner\n\ngo 1.23.0\n\nrequire example.com/errortarget v0.0.0\n\nreplace example.com/errortarget => " + modDir + "\n"
	if err := os.WriteFile(filepath.Join(runnerDir, "go.mod"), []byte(runnerMod), 0o644); err != nil {
		t.Fatalf("write runner go.mod: %v", err)
	}

	binPath := filepath.Join(runnerDir, "runner.bin")
	build := exec.Command("go", "build", "-buildvcs=false", "-overlay", manifestPath, "-o", binPath, ".")
	build.Dir = runnerDir
	build.Env = append(os.Environ(), "GOFLAGS=")
	var buildErr bytes.Buffer
	build.Stderr = &buildErr
	if err := build.Run(); err != nil {
		got, _ := os.ReadFile(wrapperPath)
		t.Fatalf("runner build failed: %v\nstderr: %s\nwrapper:\n%s", err, buildErr.String(), got)
	}
	run := exec.Command(binPath)
	var runOut, runErr bytes.Buffer
	run.Stdout = &runOut
	run.Stderr = &runErr
	if err := run.Run(); err != nil {
		t.Fatalf("runner failed: %v\nstdout: %s\nstderr: %s", err, runOut.String(), runErr.String())
	}
	if got := strings.TrimSpace(runOut.String()); got != "ok" {
		t.Errorf("runner stdout = %q, want %q\nstderr: %s", got, "ok", runErr.String())
	}
}
