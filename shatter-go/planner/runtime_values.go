package planner

import (
	"encoding/json"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"github.com/shatter-dev/shatter/shatter-go/runtimeval"
)

// RuntimeValue is a single registered candidate expression for a Go parameter
// type that cannot be expressed as a JSON literal. It is consulted by the
// parameter planner as a fallback to the primitive value families (str-hy9b.F2).
//
// The Expression field carries Go source that a wrapper generator pastes
// verbatim at the argument position. Imports declare the package paths the
// generator must add to the wrapper file. SideEffectClass is the coarse
// policy class this expression implies when used as an argument (defaults to
// ClassPure).
//
// The registry data lives in `shatter-go/runtimeval` (str-gxjs.1) so the
// wrapper package can also consume it without forming an import cycle
// through planner → protocol → build → wrapper.
type RuntimeValue struct {
	// Expression is the Go source expression (e.g. `context.Background()`).
	Expression string
	// TypeHint is the Go source spelling of the parameter type the expression
	// satisfies (e.g. "context.Context"). Matches the registry key.
	TypeHint string
	// Imports lists the unique package import paths referenced by Expression,
	// sorted for determinism.
	Imports []string
	// SideEffectClass is the policy-gate class this expression contributes.
	// Registry entries default to ClassPure.
	SideEffectClass protocol.SideEffectClass
}

// LookupRuntimeValue returns the ordered runtime-value candidates registered
// for the given Go type spelling. The returned slice is a copy; callers may
// mutate it freely. Imports on each entry are sorted and deduplicated.
//
// Unknown type spellings return nil. The lookup is case-sensitive and
// matches the exact spelling; it does not strip aliases or interface
// wrappers.
func LookupRuntimeValue(typeName string) []RuntimeValue {
	cands := runtimeval.Lookup(typeName)
	if len(cands) == 0 {
		return nil
	}
	out := make([]RuntimeValue, len(cands))
	for i, c := range cands {
		out[i] = RuntimeValue{
			Expression: c.Expression,
			TypeHint:   c.TypeHint,
			Imports:    c.Imports,
		}
		if c.SideEffectClass != "" {
			out[i].SideEffectClass = protocol.SideEffectClass(c.SideEffectClass)
		} else {
			out[i].SideEffectClass = protocol.ClassPure
		}
	}
	return out
}

// RegisteredRuntimeValueTypes returns the sorted list of type spellings the
// registry currently recognizes. Intended for diagnostics and tests.
func RegisteredRuntimeValueTypes() []string {
	return runtimeval.RegisteredTypes()
}

// runtimeValueTypeName extracts the Go-source type spelling from a ParamInfo
// for registry lookup. Returns the empty string when the parameter has no
// type-name hint (in which case the registry cannot be consulted).
func runtimeValueTypeName(p protocol.ParamInfo) string {
	if p.TypeName == nil {
		return ""
	}
	return strings.TrimSpace(*p.TypeName)
}

// runtimeValuePlans returns ValuePlans for the registry match on p, or nil
// when p has no matching registry entry. maxPlans caps the returned slice.
// Each ValuePlan carries ParamIndex/ParamName set from the caller,
// Kind=ValuePlanKindRuntimeValue, TypeHint=RuntimeValue.TypeHint, and
// Literal=JSON-encoded Go expression.
func runtimeValuePlans(paramIndex int, p protocol.ParamInfo, maxPlans int) []protocol.ValuePlan {
	typeName := runtimeValueTypeName(p)
	if typeName == "" {
		return nil
	}
	candidates := LookupRuntimeValue(typeName)
	if len(candidates) == 0 {
		return nil
	}
	if maxPlans > 0 && len(candidates) > maxPlans {
		candidates = candidates[:maxPlans]
	}
	plans := make([]protocol.ValuePlan, 0, len(candidates))
	for _, rv := range candidates {
		literal, err := json.Marshal(rv.Expression)
		if err != nil {
			// json.Marshal on a string cannot fail for valid UTF-8; skip
			// defensively rather than crash the planner.
			continue
		}
		plans = append(plans, protocol.ValuePlan{
			ParamIndex: paramIndex,
			ParamName:  p.Name,
			Kind:       protocol.ValuePlanKindRuntimeValue,
			Literal:    literal,
			TypeHint:   rv.TypeHint,
		})
	}
	return plans
}

// generatorPlans returns ValuePlans for the runtime-value registry entry
// named by typeName (the generator name supplied via
// ParamPlanOptions.GeneratorsByName). Returns nil when the registry has no
// matching entry — callers should treat that as a configuration error
// because the parameter has no other planning path once a generator was
// explicitly named.
//
// The encoding mirrors runtimeValuePlans (Kind=ValuePlanKindRuntimeValue,
// Literal=JSON-encoded Go expression, TypeHint=registered type spelling).
// maxPlans caps the returned slice.
func generatorPlans(paramIndex int, p protocol.ParamInfo, typeName string, maxPlans int) []protocol.ValuePlan {
	candidates := LookupRuntimeValue(typeName)
	if len(candidates) == 0 {
		return nil
	}
	if maxPlans > 0 && len(candidates) > maxPlans {
		candidates = candidates[:maxPlans]
	}
	plans := make([]protocol.ValuePlan, 0, len(candidates))
	for _, rv := range candidates {
		literal, err := json.Marshal(rv.Expression)
		if err != nil {
			continue
		}
		plans = append(plans, protocol.ValuePlan{
			ParamIndex: paramIndex,
			ParamName:  p.Name,
			Kind:       protocol.ValuePlanKindRuntimeValue,
			Literal:    literal,
			TypeHint:   rv.TypeHint,
		})
	}
	return plans
}
