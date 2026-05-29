package protocol

import (
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"pgregory.net/rapid"
)

func TestScanConstructorsFindsNamedCandidates(t *testing.T) {
	ldr, cleanup, err := newTransientLoader()
	if err != nil {
		t.Fatalf("newTransientLoader: %v", err)
	}
	t.Cleanup(cleanup)

	_, callerFile, _, _ := runtime.Caller(0)
	absPath, err := filepath.Abs(filepath.Join(filepath.Dir(callerFile), "testdata", "constructors.go"))
	if err != nil {
		t.Fatalf("abs path: %v", err)
	}

	pkg, err := loadPackageForAnalysis(ldr, absPath)
	if err != nil {
		t.Fatalf("loadPackageForAnalysis: %v", err)
	}

	candidates := ScanConstructors(pkg)

	byName := make(map[string]ConstructorCandidate, len(candidates))
	for _, c := range candidates {
		byName[c.FuncName] = c
	}

	// --- NewService ---
	svc, ok := byName["NewService"]
	if !ok {
		t.Errorf("NewService not found; got: %v", candidateFuncNames(candidates))
	} else {
		if svc.TargetType != "Service" {
			t.Errorf("NewService.TargetType = %q, want Service", svc.TargetType)
		}
		if len(svc.Parameters) != 1 {
			t.Errorf("NewService.Parameters len = %d, want 1", len(svc.Parameters))
		} else if svc.Parameters[0].Name != "deps" {
			t.Errorf("NewService.Parameters[0].Name = %q, want deps", svc.Parameters[0].Name)
		}
		if svc.ReturnsError {
			t.Errorf("NewService.ReturnsError = true, want false")
		}
		// str-jeen.49: NewService returns *Service, so ReturnsPointer=true.
		if !svc.ReturnsPointer {
			t.Errorf("NewService.ReturnsPointer = false, want true (returns *Service)")
		}
	}

	// --- MustNewClient ---
	cli, ok := byName["MustNewClient"]
	if !ok {
		t.Errorf("MustNewClient not found; got: %v", candidateFuncNames(candidates))
	} else {
		if cli.TargetType != "Client" {
			t.Errorf("MustNewClient.TargetType = %q, want Client", cli.TargetType)
		}
		if len(cli.Parameters) != 0 {
			t.Errorf("MustNewClient.Parameters len = %d, want 0", len(cli.Parameters))
		}
		if cli.ReturnsError {
			t.Errorf("MustNewClient.ReturnsError = true, want false")
		}
	}
}

// TestScanConstructorsRecordsReturnKind is the str-jeen.49 regression: a
// constructor's return kind (pointer vs value) must be preserved on the
// ConstructorCandidate so wrapper generation can choose the correct
// dereference shape. Pre-fix the pointer was silently unwrapped in
// samePackageTypeName and the kind was lost.
func TestScanConstructorsRecordsReturnKind(t *testing.T) {
	ldr, cleanup, err := newTransientLoader()
	if err != nil {
		t.Fatalf("newTransientLoader: %v", err)
	}
	t.Cleanup(cleanup)

	tmpFile := filepath.Join(t.TempDir(), "ret.go")
	src := `package testdata

type Registry struct{ n int }
type Service struct{ n int }

// DefaultRegistry returns a value type — the failure case from Zolem.
func DefaultRegistry() Registry { return Registry{} }

// NewService returns a pointer type.
func NewService() *Service { return &Service{} }
`
	if err := os.WriteFile(tmpFile, []byte(src), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	pkg, err := loadPackageForAnalysis(ldr, tmpFile)
	if err != nil {
		t.Fatalf("loadPackageForAnalysis: %v", err)
	}

	candidates := ScanConstructors(pkg)
	byName := map[string]ConstructorCandidate{}
	for _, c := range candidates {
		byName[c.FuncName] = c
	}

	def, ok := byName["DefaultRegistry"]
	if !ok {
		t.Fatalf("DefaultRegistry not found in candidates: %v", candidateFuncNames(candidates))
	}
	if def.ReturnsPointer {
		t.Errorf("DefaultRegistry.ReturnsPointer = true, want false (returns Registry)")
	}

	svc, ok := byName["NewService"]
	if !ok {
		t.Fatalf("NewService not found in candidates: %v", candidateFuncNames(candidates))
	}
	if !svc.ReturnsPointer {
		t.Errorf("NewService.ReturnsPointer = false, want true (returns *Service)")
	}
}

func TestScanConstructorsRecordsParameterTypeNames(t *testing.T) {
	ldr, cleanup, err := newTransientLoader()
	if err != nil {
		t.Fatalf("newTransientLoader: %v", err)
	}
	t.Cleanup(cleanup)

	tmpFile := filepath.Join(t.TempDir(), "ctor_params.go")
	src := `package testdata

import "time"

type Options struct{}
type Runner struct{}
type Fixture struct{}
type Adapter struct{}

func NewAdapter(opts Options, runner *Runner, payload []byte, fixtures []Fixture, headers map[string]string, timeout time.Duration) *Adapter {
	return &Adapter{}
}
`
	if err := os.WriteFile(tmpFile, []byte(src), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	pkg, err := loadPackageForAnalysis(ldr, tmpFile)
	if err != nil {
		t.Fatalf("loadPackageForAnalysis: %v", err)
	}

	candidates := ScanConstructors(pkg)
	var found *ConstructorCandidate
	for i := range candidates {
		if candidates[i].FuncName == "NewAdapter" {
			found = &candidates[i]
			break
		}
	}
	if found == nil {
		t.Fatalf("NewAdapter not found in candidates: %v", candidateFuncNames(candidates))
	}

	wantTypes := []string{
		"Options",
		"*Runner",
		"[]byte",
		"[]Fixture",
		"map[string]string",
		"time.Duration",
	}
	if len(found.Parameters) != len(wantTypes) {
		t.Fatalf("NewAdapter.Parameters len = %d, want %d: %+v", len(found.Parameters), len(wantTypes), found.Parameters)
	}
	for i, want := range wantTypes {
		param := found.Parameters[i]
		if param.TypeName == nil {
			t.Fatalf("param %d %q TypeName = nil, want %q", i, param.Name, want)
		}
		if *param.TypeName != want {
			t.Errorf("param %d %q TypeName = %q, want %q", i, param.Name, *param.TypeName, want)
		}
	}
}

func TestScanConstructorsExcludesMethods(t *testing.T) {
	ldr, cleanup, err := newTransientLoader()
	if err != nil {
		t.Fatalf("newTransientLoader: %v", err)
	}
	t.Cleanup(cleanup)

	_, callerFile, _, _ := runtime.Caller(0)
	absPath, err := filepath.Abs(filepath.Join(filepath.Dir(callerFile), "testdata", "targets.go"))
	if err != nil {
		t.Fatalf("abs path: %v", err)
	}

	pkg, err := loadPackageForAnalysis(ldr, absPath)
	if err != nil {
		t.Fatalf("loadPackageForAnalysis: %v", err)
	}

	// targets.go has value-receiver method Add and pointer-receiver method Reset —
	// neither should appear as a constructor candidate.
	candidates := ScanConstructors(pkg)
	for _, c := range candidates {
		if c.FuncName == "Add" || c.FuncName == "Reset" {
			t.Errorf("method %q should not appear in constructor candidates", c.FuncName)
		}
	}
}

func TestScanConstructorsReturnsErrorVariant(t *testing.T) {
	ldr, cleanup, err := newTransientLoader()
	if err != nil {
		t.Fatalf("newTransientLoader: %v", err)
	}
	t.Cleanup(cleanup)

	tmpFile := filepath.Join(t.TempDir(), "factory.go")
	src := `package testdata

type Widget struct{ id int }

func NewWidget(id int) (*Widget, error) {
	return &Widget{id: id}, nil
}
`
	if err := os.WriteFile(tmpFile, []byte(src), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	pkg, err := loadPackageForAnalysis(ldr, tmpFile)
	if err != nil {
		t.Fatalf("loadPackageForAnalysis: %v", err)
	}

	candidates := ScanConstructors(pkg)
	var found bool
	for _, c := range candidates {
		if c.FuncName != "NewWidget" {
			continue
		}
		found = true
		if c.TargetType != "Widget" {
			t.Errorf("NewWidget.TargetType = %q, want Widget", c.TargetType)
		}
		if !c.ReturnsError {
			t.Errorf("NewWidget.ReturnsError = false, want true")
		}
		if len(c.Parameters) != 1 || c.Parameters[0].Name != "id" {
			t.Errorf("NewWidget.Parameters = %v, want [{id int}]", c.Parameters)
		}
	}
	if !found {
		t.Errorf("NewWidget not found in candidates: %v", candidateFuncNames(candidates))
	}
}

func TestConstructorCandidateJSONRoundtrip(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		c := genConstructorCandidate().Draw(rt, "candidate")
		data, err := json.Marshal(c)
		if err != nil {
			rt.Fatalf("marshal: %v", err)
		}
		var got ConstructorCandidate
		if err := json.Unmarshal(data, &got); err != nil {
			rt.Fatalf("unmarshal: %v", err)
		}
		if got.FuncName != c.FuncName {
			rt.Errorf("FuncName: got %q, want %q", got.FuncName, c.FuncName)
		}
		if got.TargetType != c.TargetType {
			rt.Errorf("TargetType: got %q, want %q", got.TargetType, c.TargetType)
		}
		if got.ReturnsError != c.ReturnsError {
			rt.Errorf("ReturnsError: got %v, want %v", got.ReturnsError, c.ReturnsError)
		}
		if got.ReturnsPointer != c.ReturnsPointer {
			rt.Errorf("ReturnsPointer: got %v, want %v", got.ReturnsPointer, c.ReturnsPointer)
		}
	})
}

func genConstructorCandidate() *rapid.Generator[ConstructorCandidate] {
	return rapid.Custom(func(rt *rapid.T) ConstructorCandidate {
		prefix := rapid.SampledFrom([]string{"New", "MustNew", "Default"}).Draw(rt, "prefix")
		suffix := rapid.StringMatching(`[A-Z][a-z]{1,8}`).Draw(rt, "suffix")
		return ConstructorCandidate{
			FuncName:       prefix + suffix,
			TargetType:     rapid.StringMatching(`[A-Z][a-z]{2,8}`).Draw(rt, "type"),
			Parameters:     []ParamInfo{},
			ReturnsError:   rapid.Bool().Draw(rt, "returns_error"),
			ReturnsPointer: rapid.Bool().Draw(rt, "returns_pointer"),
		}
	})
}

func candidateFuncNames(cs []ConstructorCandidate) []string {
	names := make([]string, len(cs))
	for i, c := range cs {
		names[i] = c.FuncName
	}
	return names
}
