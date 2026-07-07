package wrapper_test

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/runtimeval"
	"github.com/shatter-dev/shatter/shatter-go/wrapper"
)

// TestWrapper_SymbolicParamsDeriveFromRegistry is the str-ijtww single-source
// regression: for every symbolic type the runtimeval registry declares, the
// wrapper must (a) consume exactly one symbolic input slot for the param
// (leaving it unbound to any fixed runtime value), (b) emit the registry's
// construction statements verbatim, and (c) reference the registry's declared
// imports. This ties the wrapper's slot consumption to the same source the
// analyzer's slot allocation keys off, so a type added to the registry can
// never be handled by only one layer (which would shift every later param's
// input index).
func TestWrapper_SymbolicParamsDeriveFromRegistry(t *testing.T) {
	symbolicTypes := runtimeval.SymbolicTypes()
	if len(symbolicTypes) == 0 {
		t.Fatal("no symbolic types registered; registry regressed")
	}
	for _, goType := range symbolicTypes {
		t.Run(goType, func(t *testing.T) {
			cand, ok := runtimeval.LookupSymbolic(goType)
			if !ok {
				t.Fatalf("LookupSymbolic(%q) not found", goType)
			}
			targets := []wrapper.WrapperTarget{{
				ID:         "example.com/pkg:Handle",
				SymbolName: "Handle",
				Kind:       wrapper.TargetKindFunction,
				Parameters: []wrapper.WrapperParam{{Name: "r", GoType: goType}},
				HasResult:  false,
				Imports:    cand.Imports,
			}}
			src := wrapper.GenerateWrapper("pkg", targets, nil)

			// (a) the symbolic body is decoded from the param's single input slot.
			if !strings.Contains(src, "json.Unmarshal(_shatterInputs[0]") {
				t.Errorf("wrapper does not decode symbolic body from _shatterInputs[0]; slot not consumed\nsource:\n%s", src)
			}
			// (b) each registry construction statement appears verbatim, rendered
			// with the param name and the body slot variable.
			for _, stmt := range cand.Construction {
				want := fmt.Sprintf(stmt, "r", "_shatterReqBody0")
				if !strings.Contains(src, want) {
					t.Errorf("wrapper missing registry construction line %q\nsource:\n%s", want, src)
				}
			}
			// (c) the registry's imports are present.
			for _, imp := range cand.Imports {
				if !strings.Contains(src, imp) {
					t.Errorf("wrapper missing registry-declared import %q\nsource:\n%s", imp, src)
				}
			}
		})
	}
}

// TestWrapper_SymbolicHTTPRequestParam is the str-e41w compile-and-run
// regression. A direct *http.Request parameter must (a) generate the
// symbolic-body construction (httptest.NewRequest around the string input
// slot, stub auth headers), (b) compile against the target module, and
// (c) at run time deliver the body, method, and headers the generated code
// promises — including the null slot the Rust core materializes for
// runtime_value plans, which must yield an empty body rather than an error.
func TestWrapper_SymbolicHTTPRequestParam(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package reqtarget

import (
	"io"
	"net/http"
)

// Describe reports what the handler observed: method, auth header
// presence, and the raw body. Mirrors the decode-guard shape of real
// HTTP handlers.
func Describe(r *http.Request) string {
	body, err := io.ReadAll(r.Body)
	if err != nil {
		return "readerr"
	}
	out := r.Method
	if r.Header.Get("x-api-key") != "" {
		out += "+key"
	}
	if r.Header.Get("Authorization") != "" {
		out += "+bearer"
	}
	if r.Header.Get("x-goog-api-key") != "" {
		out += "+goog"
	}
	return out + ":" + string(body)
}
`
	if err := os.WriteFile(filepath.Join(modDir, "reqtarget.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write reqtarget.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/reqtarget\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	targets := []wrapper.WrapperTarget{
		{
			ID:         "example.com/reqtarget:Describe",
			SymbolName: "Describe",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				{Name: "r", GoType: "*http.Request"},
			},
			HasResult:    true,
			ResultGoType: "string",
			ResultCount:  1,
			Imports:      []string{"net/http", "net/http/httptest", "strings"},
		},
	}

	src := wrapper.GenerateWrapper("reqtarget", targets, nil)
	for _, want := range []string{
		"httptest.NewRequest(",
		"strings.NewReader(",
		`.Header.Set("x-api-key", "shatter")`,
		`.Header.Set("Authorization", "Bearer shatter")`,
		`.Header.Set("x-goog-api-key", "shatter")`,
	} {
		if !strings.Contains(src, want) {
			t.Errorf("generated wrapper missing %q\nsource:\n%s", want, src)
		}
	}

	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "reqtarget", targets, nil)
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

	const runnerSrc = `package main

import (
	"encoding/json"
	"fmt"
	"os"

	reqtarget "example.com/reqtarget"
)

func main() {
	inputs := []json.RawMessage{
		json.RawMessage(` + "`" + `"{\"model\":\"m\"}"` + "`" + `), // JSON body as string literal
		json.RawMessage("null"),                    // core's runtime_value null slot
		json.RawMessage(` + "`" + `""` + "`" + `),  // explicit empty body
	}
	want := []string{
		"POST+key+bearer+goog:{\"model\":\"m\"}",
		"POST+key+bearer+goog:",
		"POST+key+bearer+goog:",
	}
	for i, in := range inputs {
		got, err := reqtarget.ShatterInvoke(reqtarget.PlanDescriptor{TargetID: "example.com/reqtarget:Describe"}, []json.RawMessage{in})
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
	runnerDir := filepath.Join(modDir, "cmd", "runner")
	if err := os.MkdirAll(runnerDir, 0o755); err != nil {
		t.Fatalf("mkdir runner: %v", err)
	}
	if err := os.WriteFile(filepath.Join(runnerDir, "main.go"), []byte(runnerSrc), 0o644); err != nil {
		t.Fatalf("write runner main.go: %v", err)
	}

	run := exec.Command("go", "run", "-buildvcs=false", "-overlay", manifestPath, "./cmd/runner")
	run.Dir = modDir
	run.Env = append(os.Environ(), "GOFLAGS=")
	var runOut, runErr bytes.Buffer
	run.Stdout = &runOut
	run.Stderr = &runErr
	if err := run.Run(); err != nil {
		got, _ := os.ReadFile(wrapperPath)
		t.Fatalf("go run failed: %v\nstderr: %s\ngenerated wrapper:\n%s", err, runErr.String(), got)
	}
	if !strings.Contains(runOut.String(), "ok") {
		t.Fatalf("runner output = %q, want ok", runOut.String())
	}
}
