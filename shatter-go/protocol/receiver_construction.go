package protocol

import (
	"go/types"

	"golang.org/x/tools/go/packages"
)

// ReceiverRequiresConstruction reports whether the receiver type of `target`
// has a zero value that is unlikely to exercise meaningful behavior — i.e.
// the underlying struct carries unexported reference-typed fields that a
// constructor is expected to initialize.
//
// The check is conservative: it returns true only when at least one
// unexported field is a map, channel, function, interface, or pointer type.
// Slices and arrays are intentionally excluded — zero-value slices range
// safely as empty sequences and rarely produce false-meaningful behavior.
// Numeric / string / bool fields are excluded — their zero value is well
// defined.
//
// Returns false for nil package, nil target, free-function targets,
// interface receivers (already short-circuited upstream), generic-unbound
// receivers, named primitives, or any case where the receiver type cannot
// be resolved to a struct shape.
//
// Callers wire the result through PlanOptions.ReceiverRequiresConstruction
// (and through the synthesizeExecuteReceiverKind path in handler.go) so
// that PlanReceivers emits an UnsatisfiedRequirementKindRequiresConstruction
// instead of a fallback zero-value plan when no real strategy applies
// (str-g7h7).
func ReceiverRequiresConstruction(pkg *packages.Package, target *DiscoveredTarget) bool {
	if pkg == nil || pkg.TypesInfo == nil || target == nil || target.Receiver == nil {
		return false
	}
	if target.Receiver.IsInterface {
		return false
	}
	scope := pkg.Types.Scope()
	if scope == nil {
		return false
	}
	obj := scope.Lookup(target.Receiver.TypeName)
	if obj == nil {
		return false
	}
	named, ok := obj.Type().(*types.Named)
	if !ok {
		return false
	}
	st, ok := named.Underlying().(*types.Struct)
	if !ok {
		return false
	}
	for i := 0; i < st.NumFields(); i++ {
		f := st.Field(i)
		if f.Exported() {
			// Exported fields can be initialized by callers via composite
			// literals; the receiver planner already emits the
			// composite-literal strategy for that case. We only flag types
			// whose required state is hidden behind unexported fields.
			continue
		}
		if fieldRequiresInitialization(f.Type()) {
			return true
		}
	}
	return false
}

// fieldRequiresInitialization reports whether a field type is a reference
// type whose zero value (nil) is dangerous to use without prior
// initialization. The set mirrors the canonical "nil panics" inventory:
// map reads/writes never panic on nil but writes do; channel sends/receives
// block forever; function calls panic; interface method calls panic;
// pointer dereferences panic.
func fieldRequiresInitialization(t types.Type) bool {
	switch t.Underlying().(type) {
	case *types.Map, *types.Chan, *types.Signature, *types.Interface, *types.Pointer:
		return true
	}
	return false
}
