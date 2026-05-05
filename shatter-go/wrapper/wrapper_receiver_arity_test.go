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
	for _, p := range plans {
		if p.ReceiverKind == "constructor:NewAdapter" {
			t.Fatalf("planner emitted constructor:NewAdapter for a parameterful ctor; the wrapper drops this case (str-qo1.14) and dispatch would surface as runtime \"unknown receiver kind\"; plans=%+v", plans)
		}
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
		{FuncName: "NewAdapter", TargetType: "Adapter", HasParams: true},
	}

	src := wrapper.GenerateWrapper("arity", []wrapper.WrapperTarget{wrapperTarget}, wrapperCtors)
	if strings.Contains(src, "NewAdapter()") {
		t.Errorf("wrapper emitted parameterless call to NewAdapter; source:\n%s", src)
	}
	if strings.Contains(src, "constructor:NewAdapter") {
		t.Errorf("wrapper emitted constructor:NewAdapter case for parameterful ctor; source:\n%s", src)
	}
	for _, p := range plans {
		caseLiteral := "case \"" + p.ReceiverKind + "\""
		if !strings.Contains(src, caseLiteral) {
			t.Errorf("wrapper missing switch case for plan ReceiverKind=%q (would dispatch to default and fail at runtime); source:\n%s", p.ReceiverKind, src)
		}
	}

	const targetSrc = `package arity

type Adapter struct{ endpoint string }

func NewAdapter(endpoint string) *Adapter { return &Adapter{endpoint: endpoint} }

func (a *Adapter) Run() {}
`
	compileWrapperFixture(t, "arity", targetSrc, []wrapper.WrapperTarget{wrapperTarget}, wrapperCtors)
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
