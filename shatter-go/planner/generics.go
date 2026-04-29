package planner

import (
	"fmt"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// Generic instantiation defaults (str-hy9b.F5).
//
// When a target carries a type parameter `T` with a recognized constraint, the
// planner instantiates `T` against a small fixed set of concrete Go types.
// Instantiation choice is constraint-aware: `any` and `comparable` admit all
// five defaults; `cmp.Ordered` excludes `bool`. Constraints we cannot decode
// (interfaces, type-set unions, unknown identifiers) surface as
// UnsatisfiedRequirementKindGenericUnconstrained with a documented reason.

// GenericTypeName names one concrete instantiation candidate. The TypeName is
// the Go source spelling that the wrapper generator embeds in the
// instantiated call site (e.g., `Identity[string](x)` for TypeName="string").
type GenericTypeName = string

const (
	genericTypeString  GenericTypeName = "string"
	genericTypeInt     GenericTypeName = "int"
	genericTypeInt64   GenericTypeName = "int64"
	genericTypeFloat64 GenericTypeName = "float64"
	genericTypeBool    GenericTypeName = "bool"
)

// genericAnyDefaults is the canonical five-type instantiation set used for
// `any` and `comparable` constraints. Order is deterministic and stable
// across calls so callers (and the matrix-cross-product builder) see a fixed
// priority.
var genericAnyDefaults = []GenericTypeName{
	genericTypeString,
	genericTypeInt,
	genericTypeBool,
	genericTypeInt64,
	genericTypeFloat64,
}

// genericOrderedDefaults restricts genericAnyDefaults to the types that
// satisfy `cmp.Ordered` (and, equivalently, `golang.org/x/exp/constraints`
// `Ordered`). bool is the only default that is not ordered.
var genericOrderedDefaults = []GenericTypeName{
	genericTypeString,
	genericTypeInt,
	genericTypeInt64,
	genericTypeFloat64,
}

// Recognized constraint identifiers.
const (
	constraintAny          = "any"
	constraintInterfaceAny = "interface{}"
	constraintComparable   = "comparable"
	constraintCmpOrdered   = "cmp.Ordered"
	// constraintsOrdered is the legacy spelling from
	// golang.org/x/exp/constraints (and its predecessor in the standard
	// library proposals). Treated identically to `cmp.Ordered`.
	constraintsOrdered = "constraints.Ordered"
)

// GenericInstantiation is one concrete instantiation candidate for a type
// parameter. The wrapper generator emits one specialized call site per
// instantiation; the cross-product with parameter ValuePlans is built by the
// caller, not here.
type GenericInstantiation struct {
	// TypeParamName is the type parameter being instantiated (e.g. "T").
	TypeParamName string
	// TypeName is the concrete Go type to substitute in (e.g. "string").
	TypeName GenericTypeName
}

// PlanGenericInstantiations returns the concrete-type instantiation set for a
// single type parameter, given the source spelling of the constraint.
//
// Returns:
//   - (nonempty insts, nil): the constraint was recognized and produced a
//     non-empty set of concrete instantiations.
//   - (nil, &unsat): the constraint is real Go but the planner cannot
//     decompose it into a concrete instantiation set (interface constraints,
//     custom type-set unions, unknown stdlib identifiers). The unsatisfied
//     reason names the constraint and the documented limitation.
//
// AC: `func[T any](t T)` → 5 plans (string, int, bool, int64, float64);
// `func[T comparable](t T)` → 5 plans (every default is comparable);
// `cmp.Ordered` → 4 plans (bool excluded); unknown constraint → unsatisfied.
func PlanGenericInstantiations(targetID, typeParamName, constraint string) ([]GenericInstantiation, *protocol.UnsatisfiedRequirement) {
	rawConstraint := constraint
	normalized := strings.TrimSpace(constraint)

	var typeNames []GenericTypeName
	switch normalized {
	case constraintAny, constraintInterfaceAny, "", constraintComparable:
		// `any`, `interface{}`, and the empty constraint admit any type;
		// `comparable` is also satisfied by all five defaults (string, int,
		// int64, float64, bool are all comparable in Go).
		typeNames = genericAnyDefaults
	case constraintCmpOrdered, constraintsOrdered:
		typeNames = genericOrderedDefaults
	default:
		return nil, &protocol.UnsatisfiedRequirement{
			Kind:     protocol.UnsatisfiedRequirementKindGenericUnconstrained,
			TargetID: targetID,
			Detail:   genericUnsupportedDetail(typeParamName, rawConstraint),
		}
	}

	insts := make([]GenericInstantiation, 0, len(typeNames))
	for _, tn := range typeNames {
		insts = append(insts, GenericInstantiation{
			TypeParamName: typeParamName,
			TypeName:      tn,
		})
	}
	return insts, nil
}

// PlanGenericTypeArgSets returns the ordered concrete type-argument tuples for
// a target's type parameters. Non-generic targets return one empty tuple so
// callers can treat generic and non-generic planning uniformly.
func PlanGenericTypeArgSets(targetID string, typeParams []protocol.TypeParamInfo) ([][]GenericTypeName, *protocol.UnsatisfiedRequirement) {
	if len(typeParams) == 0 {
		return [][]GenericTypeName{{}}, nil
	}

	sets := [][]GenericTypeName{{}}
	for _, tp := range typeParams {
		insts, unsat := PlanGenericInstantiations(targetID, tp.Name, tp.Constraint)
		if unsat != nil {
			return nil, unsat
		}
		if len(insts) == 0 {
			return nil, &protocol.UnsatisfiedRequirement{
				Kind:     protocol.UnsatisfiedRequirementKindGenericUnconstrained,
				TargetID: targetID,
				Detail:   genericUnsupportedDetail(tp.Name, tp.Constraint),
			}
		}

		next := make([][]GenericTypeName, 0, len(sets)*len(insts))
		for _, prefix := range sets {
			for _, inst := range insts {
				tuple := append(append([]GenericTypeName{}, prefix...), inst.TypeName)
				next = append(next, tuple)
			}
		}
		sets = next
	}
	return sets, nil
}

// genericUnsupportedDetail formats the human-readable reason emitted on
// UnsatisfiedRequirement.Detail for an unrecognized constraint. The detail
// names both the type parameter and the offending constraint so the operator
// can see exactly what the planner gave up on.
func genericUnsupportedDetail(typeParamName, constraint string) string {
	if typeParamName == "" {
		typeParamName = "T"
	}
	return fmt.Sprintf(
		"type parameter %s has constraint %q: only `any`, `interface{}`, `comparable`, and `cmp.Ordered` "+
			"(and `constraints.Ordered`) are recognized; interface and type-set constraints are not yet decomposed",
		typeParamName, constraint,
	)
}
