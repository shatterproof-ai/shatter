package protocol

import (
	"path/filepath"
	"runtime"
	"testing"

	"golang.org/x/tools/go/packages"
)

// TestReceiverRequiresConstruction_DetectsNilFieldStruct asserts the
// str-g7h7 heuristic: a struct whose unexported fields are reference types
// (maps, channels, interfaces, function values, pointers) is flagged so the
// receiver planner refuses the fallback zero-value plan.
func TestReceiverRequiresConstruction_DetectsNilFieldStruct(t *testing.T) {
	pkg := loadConstructionTestdata(t)

	target := findMethodTarget(t, pkg, "ListProfiles", "LocalControlPlane")
	if !ReceiverRequiresConstruction(pkg, &target) {
		t.Fatalf("ReceiverRequiresConstruction(LocalControlPlane) = false, want true")
	}
}

// Negative control: a struct with only primitive fields must not be flagged
// — the existing fallback zero-value plan stays usable.
func TestReceiverRequiresConstruction_AllowsPrimitiveStruct(t *testing.T) {
	pkg := loadConstructionTestdata(t)

	target := findMethodTarget(t, pkg, "Describe", "PrimitiveOnly")
	if ReceiverRequiresConstruction(pkg, &target) {
		t.Fatalf("ReceiverRequiresConstruction(PrimitiveOnly) = true, want false")
	}
}

func loadConstructionTestdata(t *testing.T) *packages.Package {
	t.Helper()
	ldr, cleanup, err := newTransientLoader()
	if err != nil {
		t.Fatalf("newTransientLoader: %v", err)
	}
	t.Cleanup(cleanup)
	_, callerFile, _, _ := runtime.Caller(0)
	absPath, err := filepath.Abs(filepath.Join(filepath.Dir(callerFile), "testdata", "requires_construction.go"))
	if err != nil {
		t.Fatalf("abs path: %v", err)
	}
	pkg, err := loadPackageForAnalysis(ldr, absPath)
	if err != nil {
		t.Fatalf("loadPackageForAnalysis: %v", err)
	}
	if pkg == nil {
		t.Fatalf("nil pkg")
	}
	return pkg
}

func findMethodTarget(t *testing.T, pkg *packages.Package, method, recvType string) DiscoveredTarget {
	t.Helper()
	fn := findFuncDeclByBareName(pkg, method)
	if fn == nil {
		t.Fatalf("findFuncDeclByBareName(%q) = nil", method)
	}
	tgt := BuildDiscoveredTarget(pkg.Fset, fn, pkg.TypesInfo, pkg.PkgPath, pkg.Name, "")
	if tgt.Receiver == nil || tgt.Receiver.TypeName != recvType {
		t.Fatalf("target.Receiver = %+v, want type %q", tgt.Receiver, recvType)
	}
	return tgt
}
