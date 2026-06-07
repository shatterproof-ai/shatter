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

// TestWrapper_DurationParam is the str-is5g regression. A time.Duration
// parameter must (a) generate the dedicated decode block that accepts both
// integer-nanosecond JSON and the legacy `{"__complex_type":"duration",
// "ms":N}` shape, (b) compile against the target module, and (c) decode
// each form correctly through ShatterInvoke at run time.
func TestWrapper_DurationParam(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package durationtarget

import "time"

// Categorize mirrors examples/go/duration-param/duration.go. Four
// reachable branches give the wrapper-driven invocation observable
// outcomes per input form.
func Categorize(timeout time.Duration) int {
	if timeout < 0 {
		return -1
	}
	if timeout == 0 {
		return 0
	}
	if timeout < time.Second {
		return 1
	}
	return 2
}
`
	if err := os.WriteFile(filepath.Join(modDir, "durationtarget.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write durationtarget.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/durationtarget\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	targets := []wrapper.WrapperTarget{
		{
			ID:         "example.com/durationtarget:Categorize",
			SymbolName: "Categorize",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				{Name: "timeout", GoType: "time.Duration"},
			},
			HasResult:    true,
			ResultGoType: "int",
			ResultCount:  1,
			Imports:      []string{"time"},
		},
	}

	src := wrapper.GenerateWrapper("durationtarget", targets, nil)

	// Static guards on the generated source.
	mustContain := []string{
		"\"time\"",                  // import
		"var timeout time.Duration", // declared
		"_shatterDur",               // object-fallback struct
		"_shatterDur.ComplexType != \"duration\"",                     // tag check
		"timeout = time.Duration(*_shatterDur.Ms) * time.Millisecond", // ms→ns conversion
	}
	for _, want := range mustContain {
		if !strings.Contains(src, want) {
			t.Errorf("generated wrapper missing %q\nsource:\n%s", want, src)
		}
	}

	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "durationtarget", targets, nil)
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
	// ShatterInvoke with each input form and prints the result. The
	// canonical form is integer nanoseconds; the object form covers the
	// Rust core's random-explorer path.
	const runnerSrc = `package main

import (
	"encoding/json"
	"fmt"
	"os"

	durationtarget "example.com/durationtarget"
)

func main() {
	inputs := []json.RawMessage{
		json.RawMessage("0"),                                            // zero ns
		json.RawMessage("1000000000"),                                   // 1s in ns
		json.RawMessage("-86400000000000"),                              // -1 day in ns
		json.RawMessage(` + "`" + `{"__complex_type":"duration","ms":1000}` + "`" + `), // 1s via legacy object
		json.RawMessage(` + "`" + `{"__complex_type":"duration","ms":-1}` + "`" + `),   // -1ms via legacy object
		json.RawMessage("500000"),                                       // 500us in ns (< 1s, > 0)
	}
	want := []int{0, 2, -1, 2, -1, 1}
	for i, in := range inputs {
		got, err := durationtarget.ShatterInvoke(durationtarget.PlanDescriptor{TargetID: "example.com/durationtarget:Categorize"}, []json.RawMessage{in})
		if err != nil {
			fmt.Fprintf(os.Stderr, "case %d: ShatterInvoke error: %v\n", i, err)
			os.Exit(1)
		}
		g, ok := got.(int)
		if !ok {
			fmt.Fprintf(os.Stderr, "case %d: result type %T, want int\n", i, got)
			os.Exit(1)
		}
		if g != want[i] {
			fmt.Fprintf(os.Stderr, "case %d: got %d, want %d\n", i, g, want[i])
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
	runnerMod := "module example.com/durationrunner\n\ngo 1.23.0\n\nrequire example.com/durationtarget v0.0.0\n\nreplace example.com/durationtarget => " + modDir + "\n"
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

// TestWrapper_StructWithDurationField is the str-n66n regression. A struct
// parameter containing a time.Duration field must deserialize integer
// nanoseconds via plain json.Unmarshal — the wrapper does not need the
// top-level duration-specific decode block because the core emits
// ComplexKind=go_duration (not generic Duration) for nested fields, so the
// wire format is always a plain integer.
func TestWrapper_StructWithDurationField(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package structdur

import "time"

func F(cfg struct{ Delay time.Duration }) int {
	if cfg.Delay < 0 {
		return -1
	}
	if cfg.Delay == 0 {
		return 0
	}
	return 1
}
`
	if err := os.WriteFile(filepath.Join(modDir, "structdur.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write structdur.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/structdur\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	targets := []wrapper.WrapperTarget{
		{
			ID:         "example.com/structdur:F",
			SymbolName: "F",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				{Name: "cfg", GoType: "struct{ Delay time.Duration }"},
			},
			HasResult:    true,
			ResultGoType: "int",
			ResultCount:  1,
			Imports:      []string{"time"},
		},
	}

	src := wrapper.GenerateWrapper("structdur", targets, nil)

	// The struct param must NOT trigger the duration-specific decode block
	// (that's for top-level time.Duration params only). Instead, the struct
	// is deserialized as a whole; since the core emits integer nanoseconds
	// for go_duration fields, json.Unmarshal into struct{Delay time.Duration}
	// succeeds.
	if strings.Contains(src, "_shatterDur") {
		t.Errorf("generated wrapper should NOT contain duration-specific decode block for struct param\nsource:\n%s", src)
	}

	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "structdur", targets, nil)
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

	// Drive the generated wrapper: integer nanoseconds must unmarshal
	// into the struct's time.Duration field correctly.
	const runnerSrc = `package main

import (
	"encoding/json"
	"fmt"
	"os"

	structdur "example.com/structdur"
)

func main() {
	// Each input is a JSON object with a "Delay" field as integer nanoseconds.
	inputs := []json.RawMessage{
		json.RawMessage(` + "`" + `{"Delay":0}` + "`" + `),
		json.RawMessage(` + "`" + `{"Delay":1000000000}` + "`" + `),
		json.RawMessage(` + "`" + `{"Delay":-1000000000}` + "`" + `),
	}
	want := []int{0, 1, -1}
	for i, in := range inputs {
		got, err := structdur.ShatterInvoke(structdur.PlanDescriptor{TargetID: "example.com/structdur:F"}, []json.RawMessage{in})
		if err != nil {
			fmt.Fprintf(os.Stderr, "case %d: ShatterInvoke error: %v\n", i, err)
			os.Exit(1)
		}
		g, ok := got.(int)
		if !ok {
			fmt.Fprintf(os.Stderr, "case %d: result type %T, want int\n", i, got)
			os.Exit(1)
		}
		if g != want[i] {
			fmt.Fprintf(os.Stderr, "case %d: got %d, want %d\n", i, g, want[i])
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
	runnerMod := "module example.com/structdurrunner\n\ngo 1.23.0\n\nrequire example.com/structdur v0.0.0\n\nreplace example.com/structdur => " + modDir + "\n"
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

// TestWrapper_StructWithTimeFieldAcceptsDateMarker is the str-e07l
// regression. A typed parameter containing a time.Time field must accept the
// core's complex date marker before json.Unmarshal reaches time.Time's string
// decoder.
func TestWrapper_StructWithTimeFieldAcceptsDateMarker(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package structtime

import "time"

func UnixMillis(cfg struct{ Now time.Time }) int64 {
	return cfg.Now.UnixMilli()
}
`
	if err := os.WriteFile(filepath.Join(modDir, "structtime.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write structtime.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/structtime\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	targets := []wrapper.WrapperTarget{
		{
			ID:         "example.com/structtime:UnixMillis",
			SymbolName: "UnixMillis",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				{Name: "cfg", GoType: "struct{ Now time.Time }"},
			},
			HasResult:    true,
			ResultGoType: "int64",
			ResultCount:  1,
			Imports:      []string{"time"},
		},
	}

	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "structtime", targets, nil)
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

	const runnerSrc = `package main

import (
	"encoding/json"
	"fmt"
	"os"

	structtime "example.com/structtime"
)

func main() {
	got, err := structtime.ShatterInvoke(
		structtime.PlanDescriptor{TargetID: "example.com/structtime:UnixMillis"},
		[]json.RawMessage{json.RawMessage(` + "`" + `{"Now":{"__complex_type":"date","value":2147483647000}}` + "`" + `)},
	)
	if err != nil {
		fmt.Fprintf(os.Stderr, "ShatterInvoke error: %v\n", err)
		os.Exit(1)
	}
	millis, ok := got.(int64)
	if !ok {
		fmt.Fprintf(os.Stderr, "result type %T, want int64\n", got)
		os.Exit(1)
	}
	if millis != 2147483647000 {
		fmt.Fprintf(os.Stderr, "got %d, want 2147483647000\n", millis)
		os.Exit(1)
	}
	fmt.Println("ok")
}
`
	runnerDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(runnerDir, "main.go"), []byte(runnerSrc), 0o644); err != nil {
		t.Fatalf("write main.go: %v", err)
	}
	runnerMod := "module example.com/structtimerunner\n\ngo 1.23.0\n\nrequire example.com/structtime v0.0.0\n\nreplace example.com/structtime => " + modDir + "\n"
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

// TestWrapper_DurationParam_RejectsInvalidObject pins the error contract:
// a JSON object that does not carry the duration tag must surface the
// integer-decode error rather than silently producing a zero value.
func TestWrapper_DurationParam_RejectsInvalidObject(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package durationtarget

import "time"

func Categorize(timeout time.Duration) int {
	if timeout < 0 {
		return -1
	}
	if timeout == 0 {
		return 0
	}
	return 1
}
`
	if err := os.WriteFile(filepath.Join(modDir, "durationtarget.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write target: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/durationtarget\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	targets := []wrapper.WrapperTarget{
		{
			ID:           "example.com/durationtarget:Categorize",
			SymbolName:   "Categorize",
			Kind:         wrapper.TargetKindFunction,
			Parameters:   []wrapper.WrapperParam{{Name: "timeout", GoType: "time.Duration"}},
			HasResult:    true,
			ResultGoType: "int",
			ResultCount:  1,
			Imports:      []string{"time"},
		},
	}

	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "durationtarget", targets, nil)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}
	hash := wrapper.DiscoveryHash(targets, nil)
	inTreePath := filepath.Join(modDir, wrapper.WrapperFilename(hash))
	manifest := map[string]map[string]string{"Replace": {inTreePath: wrapperPath}}
	manifestJSON, _ := json.MarshalIndent(manifest, "", "  ")
	manifestPath := filepath.Join(wrapperDir, "overlay.json")
	if err := os.WriteFile(manifestPath, manifestJSON, 0o644); err != nil {
		t.Fatalf("write overlay: %v", err)
	}

	const runnerSrc = `package main

import (
	"encoding/json"
	"fmt"
	"os"
	"strings"

	durationtarget "example.com/durationtarget"
)

func main() {
	// Object without the duration tag must surface a decode error
	// (the original integer-decode UnmarshalTypeError), not a zero value.
	in := json.RawMessage(` + "`" + `{"not_duration":42}` + "`" + `)
	_, err := durationtarget.ShatterInvoke(durationtarget.PlanDescriptor{TargetID: "example.com/durationtarget:Categorize"}, []json.RawMessage{in})
	if err == nil {
		fmt.Fprintln(os.Stderr, "expected error for untagged object, got nil")
		os.Exit(1)
	}
	if !strings.Contains(err.Error(), "param timeout") {
		fmt.Fprintf(os.Stderr, "expected error to mention param name, got %v\n", err)
		os.Exit(1)
	}
	fmt.Println("ok")
}
`
	runnerDir := t.TempDir()
	if err := os.WriteFile(filepath.Join(runnerDir, "main.go"), []byte(runnerSrc), 0o644); err != nil {
		t.Fatalf("write runner: %v", err)
	}
	runnerMod := "module example.com/durationrunner\n\ngo 1.23.0\n\nrequire example.com/durationtarget v0.0.0\n\nreplace example.com/durationtarget => " + modDir + "\n"
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
		t.Fatalf("runner build failed: %v\nstderr: %s", err, buildErr.String())
	}
	run := exec.Command(binPath)
	var runOut, runErr bytes.Buffer
	run.Stdout = &runOut
	run.Stderr = &runErr
	if err := run.Run(); err != nil {
		t.Fatalf("runner failed: %v\nstdout: %s\nstderr: %s", err, runOut.String(), runErr.String())
	}
}
