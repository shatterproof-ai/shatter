// Package planner produces InvocationPlans for discovered Go targets.
package planner

import "github.com/shatter-dev/shatter/shatter-go/protocol"

// TargetClass is the classification outcome for a DiscoveredTarget.
// Use a type switch to dispatch on the concrete type.
type TargetClass interface{ targetClass() }

// DirectFunctionClass means the target is a free function callable directly.
type DirectFunctionClass struct{}

// MethodClass means the target is a method whose receiver can be constructed.
type MethodClass struct{}

// AdapterCandidateClass means the target is recognised by a registered adapter.
type AdapterCandidateClass struct{}

// UnsupportedClass means the planner cannot produce a plan for this target.
type UnsupportedClass struct{ Reason UnsupportedReason }

func (DirectFunctionClass) targetClass()   {}
func (MethodClass) targetClass()           {}
func (AdapterCandidateClass) targetClass() {}
func (UnsupportedClass) targetClass()      {}

// UnsupportedReason identifies why a target cannot be planned.
type UnsupportedReason string

const (
	UnsupportedReasonGenericUnconstrained UnsupportedReason = "generic_unconstrained"
	UnsupportedReasonInterfaceReceiver    UnsupportedReason = "interface_receiver"
	UnsupportedReasonCGoDependency        UnsupportedReason = "cgo_dependency"
	UnsupportedReasonTestOnlyVisibility   UnsupportedReason = "test_only_visibility"
)

// Classify returns the TargetClass for t.
//
// Unsupported conditions are checked first in priority order:
// generic_unconstrained > interface_receiver > cgo_dependency > test_only_visibility.
// Positive classifications follow: adapter_candidate > method > direct_function.
func Classify(t protocol.DiscoveredTarget) TargetClass {
	if t.HasTypeParams {
		if len(t.TypeParams) == 0 {
			return UnsupportedClass{Reason: UnsupportedReasonGenericUnconstrained}
		}
		if _, unsat := PlanGenericTypeArgSets(t.ID, t.TypeParams); unsat != nil {
			return UnsupportedClass{Reason: UnsupportedReasonGenericUnconstrained}
		}
	}
	if t.Kind == protocol.TargetKindMethod && t.Receiver != nil && t.Receiver.IsInterface {
		return UnsupportedClass{Reason: UnsupportedReasonInterfaceReceiver}
	}
	if t.HasCGoDep {
		return UnsupportedClass{Reason: UnsupportedReasonCGoDependency}
	}
	if t.IsTestFile {
		return UnsupportedClass{Reason: UnsupportedReasonTestOnlyVisibility}
	}
	switch t.Kind {
	case protocol.TargetKindAdapter:
		return AdapterCandidateClass{}
	case protocol.TargetKindMethod:
		return MethodClass{}
	default:
		return DirectFunctionClass{}
	}
}
