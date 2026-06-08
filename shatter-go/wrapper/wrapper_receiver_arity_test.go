package wrapper_test

// str-qo1.9 regression fixtures — pair the receiver planner against the
// wrapper code generator for two failure modes exposed by the Refute scan:
//
//   (a) Constructor arity mismatch: the discovered constructor takes
//       arguments (`NewAdapter(string)`). Pre-fix the planner emitted a
//       `constructor:NewAdapter` plan whose matching wrapper switch case
//       had been dropped by str-qo1.14, causing every execute to surface
//       as `unknown receiver kind` at runtime.
//
//   (b) Pointer receiver method without any constructor (`(*Config).Server`).
//       Pre-fix the planner returned `NoConstructor`, so no plan was ever
//       dispatched and the host-level path produced an `unknown receiver
//       kind` outcome.
//
// Both fixtures exercise the full pipeline: PlanReceivers → wrapper
// codegen → `go build` against an in-memory module via `-overlay`. They
// fail loudly if the planner emits a `constructor:*` plan that the wrapper
// drops, or if the wrapper fails to compile against the synthesised
// receiver type.

import (
	"bytes"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"github.com/shatter-dev/shatter/shatter-go/wrapper"
)

// TestReceiverArity_ParameterfulConstructor_FallbackPlanCompiles regresses
// the constructor arity mismatch (str-qo1.9 (a)). The planner is fed a
// constructor candidate that requires a non-trivial argument; it MUST NOT
// emit a `constructor:NewAdapter` plan and the wrapper code generator MUST
// produce a build that compiles end-to-end with the surviving plan.
func TestReceiverArity_ParameterfulConstructor_FallbackPlanCompiles(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	target := protocol.DiscoveredTarget{
		ID:            "example.com/arity:(*Adapter).Run",
		PackagePath:   "example.com/arity",
		PackageName:   "arity",
		SymbolName:    "Run",
		QualifiedName: "(*Adapter).Run",
		Kind:          protocol.TargetKindMethod,
		Receiver:      &protocol.ReceiverShape{TypeName: "Adapter", IsPointer: true},
	}
	parameterfulCtor := protocol.ConstructorCandidate{
		FuncName:   "NewAdapter",
		TargetType: "Adapter",
		Parameters: []protocol.ParamInfo{{
			Name: "endpoint",
			Type: protocol.TypeInfo{Kind: "str"},
		}},
	}

	plans, unsat := planner.PlanReceivers(target, planner.PlanOptions{
		SamePackageConstructors: []protocol.ConstructorCandidate{parameterfulCtor},
	})
	if unsat != nil {
		t.Fatalf("PlanReceivers returned unsatisfied: %+v", unsat)
	}
	if len(plans) == 0 {
		t.Fatalf("PlanReceivers returned no plans for pointer receiver with parameterful constructor")
	}
	// str-9b1q: parameterized constructors with satisfiable primitive params
	// ARE now emitted. Verify the plan includes constructor:NewAdapter.
	foundCtorPlan := false
	for _, p := range plans {
		if p.ReceiverKind == "constructor:NewAdapter" {
			foundCtorPlan = true
			if len(p.ConstructorParams) != 1 {
				t.Fatalf("expected 1 constructor param, got %d", len(p.ConstructorParams))
			}
		}
	}
	if !foundCtorPlan {
		t.Fatalf("planner did not emit constructor:NewAdapter for satisfiable parameterful ctor; plans=%+v", plans)
	}

	wrapperTarget := wrapper.WrapperTarget{
		ID:            target.ID,
		SymbolName:    "Run",
		Kind:          wrapper.TargetKindMethod,
		ReceiverType:  "Adapter",
		IsPointerRecv: true,
		HasResult:     false,
	}
	wrapperCtors := []wrapper.ConstructorCandidate{
		{FuncName: "NewAdapter", TargetType: "Adapter", HasParams: true,
			Parameters:     []wrapper.ConstructorParam{{Name: "endpoint", GoType: "string"}},
			ReturnsPointer: true},
	}

	src := wrapper.GenerateWrapper("arity", []wrapper.WrapperTarget{wrapperTarget}, wrapperCtors)
	// str-o5p5: parameterized constructors use the input prefix for their
	// arguments, NOT NewAdapter() (arity mismatch) or a hardcoded zero literal.
	if strings.Contains(src, "NewAdapter()") {
		t.Errorf("wrapper emitted parameterless call to NewAdapter; source:\n%s", src)
	}
	if strings.Contains(src, `NewAdapter("")`) {
		t.Errorf("wrapper hardcoded constructor zero literal; source:\n%s", src)
	}
	if !strings.Contains(src, "NewAdapter(_shatterCtorArg0)") {
		t.Errorf("wrapper did not pass decoded constructor input to NewAdapter; source:\n%s", src)
	}
	if !strings.Contains(src, "constructor:NewAdapter") {
		t.Errorf("wrapper missing constructor:NewAdapter case; source:\n%s", src)
	}
	for _, p := range plans {
		caseLiteral := "case \"" + p.ReceiverKind + "\""
		if !strings.Contains(src, caseLiteral) {
			t.Errorf("wrapper missing switch case for plan ReceiverKind=%q; source:\n%s", p.ReceiverKind, src)
		}
	}

	const targetSrc = `package arity

type Adapter struct{ endpoint string }

func NewAdapter(endpoint string) *Adapter { return &Adapter{endpoint: endpoint} }

func (a *Adapter) Run() {}
`
	compileWrapperFixture(t, "arity", targetSrc, []wrapper.WrapperTarget{wrapperTarget}, wrapperCtors)
}

func TestParameterizedConstructorZeroArgsCompileForNonStringTypes(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	wrapperTarget := wrapper.WrapperTarget{
		ID:            "example.com/ctorzero:(*Adapter).Run",
		SymbolName:    "Run",
		Kind:          wrapper.TargetKindMethod,
		ReceiverType:  "Adapter",
		IsPointerRecv: true,
		HasResult:     false,
	}
	wrapperCtors := []wrapper.ConstructorCandidate{
		{
			FuncName:       "NewAdapter",
			TargetType:     "Adapter",
			HasParams:      true,
			ReturnsPointer: true,
			Parameters: []wrapper.ConstructorParam{
				{Name: "opts", GoType: "Options"},
				{Name: "runner", GoType: "*Runner"},
				{Name: "payload", GoType: "[]byte"},
				{Name: "fixtures", GoType: "[]Fixture"},
				{Name: "headers", GoType: "map[string]string"},
				{Name: "timeout", GoType: "time.Duration"},
			},
		},
	}

	src := wrapper.GenerateWrapper("ctorzero", []wrapper.WrapperTarget{wrapperTarget}, wrapperCtors)
	bannedSubstrings := []string{
		`NewAdapter("",`,
		`, "",`,
		`NewAdapter("", "", "", "", "", "")`,
	}
	for _, banned := range bannedSubstrings {
		if strings.Contains(src, banned) {
			t.Errorf("wrapper emitted string literal fallback for non-string constructor args %q; source:\n%s", banned, src)
		}
	}

	const targetSrc = `package ctorzero

import "time"

type Options struct{}
type Runner struct{}
type Fixture struct{}
type Adapter struct{}

func NewAdapter(opts Options, runner *Runner, payload []byte, fixtures []Fixture, headers map[string]string, timeout time.Duration) *Adapter {
	return &Adapter{}
}

func (a *Adapter) Run() {}
`
	compileWrapperFixture(t, "ctorzero", targetSrc, []wrapper.WrapperTarget{wrapperTarget}, wrapperCtors)
}

func TestParameterizedConstructorDeserializesArgsFromInputPrefix(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	wrapperTarget := wrapper.WrapperTarget{
		ID:            "example.com/ctorinput:(*Adapter).Run",
		SymbolName:    "Run",
		Kind:          wrapper.TargetKindMethod,
		ReceiverType:  "Adapter",
		IsPointerRecv: true,
		Parameters: []wrapper.WrapperParam{
			{Name: "mode", GoType: "string"},
		},
		HasResult: false,
	}
	wrapperCtors := []wrapper.ConstructorCandidate{
		{
			FuncName:       "NewAdapter",
			TargetType:     "Adapter",
			HasParams:      true,
			ReturnsPointer: true,
			Parameters: []wrapper.ConstructorParam{
				{Name: "endpoint", GoType: "string"},
			},
		},
	}

	src := wrapper.GenerateWrapper("ctorinput", []wrapper.WrapperTarget{wrapperTarget}, wrapperCtors)
	if strings.Contains(src, `NewAdapter("")`) {
		t.Fatalf("wrapper hardcoded parameterized constructor zero literal; source:\n%s", src)
	}
	if !strings.Contains(src, "json.Unmarshal(_shatterInputs[0], &_shatterCtorArg0)") {
		t.Fatalf("wrapper did not decode constructor arg from input slot 0; source:\n%s", src)
	}
	if !strings.Contains(src, "NewAdapter(_shatterCtorArg0)") {
		t.Fatalf("wrapper did not pass decoded constructor arg to NewAdapter; source:\n%s", src)
	}
	if !strings.Contains(src, "json.Unmarshal(_shatterInputs[1], &mode)") {
		t.Fatalf("wrapper did not shift method arg deserialization after constructor prefix; source:\n%s", src)
	}

	const targetSrc = `package ctorinput

type Adapter struct{ endpoint string }

func NewAdapter(endpoint string) *Adapter { return &Adapter{endpoint: endpoint} }

func (a *Adapter) Run(mode string) string { return a.endpoint + ":" + mode }
`
	compileWrapperFixture(t, "ctorinput", targetSrc, []wrapper.WrapperTarget{wrapperTarget}, wrapperCtors)
}

func TestParameterizedConstructorRuntimeValueDoesNotConsumeInputSlot(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	wrapperTarget := wrapper.WrapperTarget{
		ID:            "example.com/ctorruntime:(*Adapter).Run",
		SymbolName:    "Run",
		Kind:          wrapper.TargetKindMethod,
		ReceiverType:  "Adapter",
		IsPointerRecv: true,
		Parameters: []wrapper.WrapperParam{
			{Name: "mode", GoType: "string"},
		},
		HasResult: false,
	}
	wrapperCtors := []wrapper.ConstructorCandidate{
		{
			FuncName:       "NewAdapter",
			TargetType:     "Adapter",
			HasParams:      true,
			ReturnsPointer: true,
			Parameters: []wrapper.ConstructorParam{
				{Name: "w", GoType: "http.ResponseWriter"},
				{Name: "label", GoType: "string"},
			},
		},
	}

	src := wrapper.GenerateWrapper("ctorruntime", []wrapper.WrapperTarget{wrapperTarget}, wrapperCtors)
	if !strings.Contains(src, `"net/http/httptest"`) {
		t.Fatalf("wrapper missing httptest import for constructor runtime value; source:\n%s", src)
	}
	if !strings.Contains(src, "var _shatterCtorArg0 http.ResponseWriter = httptest.NewRecorder()") {
		t.Fatalf("wrapper did not bind http.ResponseWriter constructor arg from runtime value; source:\n%s", src)
	}
	if strings.Contains(src, "json.Unmarshal(_shatterInputs[0], &_shatterCtorArg0)") {
		t.Fatalf("wrapper decoded runtime-value constructor arg from inputs; source:\n%s", src)
	}
	if !strings.Contains(src, "json.Unmarshal(_shatterInputs[0], &_shatterCtorArg1)") {
		t.Fatalf("wrapper did not decode first JSON-backed constructor arg from input slot 0; source:\n%s", src)
	}
	if !strings.Contains(src, "json.Unmarshal(_shatterInputs[1], &mode)") {
		t.Fatalf("wrapper did not shift method arg after one JSON-backed constructor arg; source:\n%s", src)
	}
	if !strings.Contains(src, "NewAdapter(_shatterCtorArg0, _shatterCtorArg1)") {
		t.Fatalf("wrapper did not pass constructor args to NewAdapter; source:\n%s", src)
	}

	const targetSrc = `package ctorruntime

import "net/http"

type Adapter struct {
	w     http.ResponseWriter
	label string
}

func NewAdapter(w http.ResponseWriter, label string) *Adapter {
	return &Adapter{w: w, label: label}
}

func (a *Adapter) Run(mode string) string { return a.label + ":" + mode }
`
	compileWrapperFixture(t, "ctorruntime", targetSrc, []wrapper.WrapperTarget{wrapperTarget}, wrapperCtors)
}

// TestReceiverArity_PointerReceiverNoCtor_FallbackPlanCompiles regresses the
// `(*Config).Server` symptom (str-qo1.9 (b)). A pointer-receiver method
// with no discovered constructor must yield an executable plan (zero-value
// fallback) rather than a NoConstructor unsatisfied requirement, and the
// wrapper must compile against the receiver type.
func TestReceiverArity_PointerReceiverNoCtor_FallbackPlanCompiles(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	target := protocol.DiscoveredTarget{
		ID:            "example.com/cfg:(*Config).Server",
		PackagePath:   "example.com/cfg",
		PackageName:   "cfg",
		SymbolName:    "Server",
		QualifiedName: "(*Config).Server",
		Kind:          protocol.TargetKindMethod,
		Receiver:      &protocol.ReceiverShape{TypeName: "Config", IsPointer: true},
	}

	plans, unsat := planner.PlanReceivers(target, planner.PlanOptions{})
	if unsat != nil {
		t.Fatalf("pointer receiver without ctor must not produce unsatisfied; got %+v", unsat)
	}
	if len(plans) != 1 {
		t.Fatalf("expected exactly one fallback plan, got %+v", plans)
	}
	if plans[0].ReceiverKind != "zero_value" {
		t.Fatalf("plan ReceiverKind=%q, want zero_value (fallback)", plans[0].ReceiverKind)
	}

	wrapperTarget := wrapper.WrapperTarget{
		ID:            target.ID,
		SymbolName:    "Server",
		Kind:          wrapper.TargetKindMethod,
		ReceiverType:  "Config",
		IsPointerRecv: true,
		HasResult:     true,
		ResultGoType:  "string",
		ResultCount:   1,
	}

	src := wrapper.GenerateWrapper("cfg", []wrapper.WrapperTarget{wrapperTarget}, nil)
	if !strings.Contains(src, "case \"zero_value\":") {
		t.Errorf("wrapper missing zero_value switch case for pointer-receiver method; source:\n%s", src)
	}

	const targetSrc = `package cfg

type Config struct{ host string }

func (c *Config) Server() string { return c.host }
`
	compileWrapperFixture(t, "cfg", targetSrc, []wrapper.WrapperTarget{wrapperTarget}, nil)
}

// compileWrapperFixture writes a synthetic Go module containing pkgSrc plus
// the generated wrapper, then runs `go build -overlay ./...` to prove the
// wrapper compiles end-to-end. It mirrors the helper pattern used by the
// other wrapper compile tests in this file but is shared by both str-qo1.9
// regression fixtures.
func compileWrapperFixture(
	t *testing.T,
	pkgName string,
	pkgSrc string,
	targets []wrapper.WrapperTarget,
	ctors []wrapper.ConstructorCandidate,
) {
	t.Helper()
	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	if err := os.WriteFile(filepath.Join(modDir, pkgName+".go"), []byte(pkgSrc), 0o644); err != nil {
		t.Fatalf("write %s.go: %v", pkgName, err)
	}
	goModContent := []byte("module example.com/" + pkgName + "\n\ngo 1.23.0\n")
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), goModContent, 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, pkgName, targets, ctors)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}

	hash := wrapper.DiscoveryHash(targets, ctors)
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
}
