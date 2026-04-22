package planner

import (
	"encoding/json"
	"sort"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
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

// runtimeValueRegistry is the default set of Go parameter types the planner
// can satisfy without user hints. Keyed by the Go source spelling of the
// parameter type, including any leading `*` for pointer types.
//
// Each entry lists candidates in priority order; earlier candidates are
// preferred. The order is stable to keep plan enumeration deterministic.
var runtimeValueRegistry = map[string][]RuntimeValue{
	"context.Context": {
		{
			Expression:      "context.Background()",
			TypeHint:        "context.Context",
			Imports:         []string{"context"},
			SideEffectClass: protocol.ClassPure,
		},
	},
	"*bytes.Buffer": {
		{
			Expression:      "&bytes.Buffer{}",
			TypeHint:        "*bytes.Buffer",
			Imports:         []string{"bytes"},
			SideEffectClass: protocol.ClassPure,
		},
	},
	"io.Reader": {
		{
			Expression:      `strings.NewReader("")`,
			TypeHint:        "io.Reader",
			Imports:         []string{"strings"},
			SideEffectClass: protocol.ClassPure,
		},
	},
	"io.Writer": {
		{
			Expression:      "&bytes.Buffer{}",
			TypeHint:        "io.Writer",
			Imports:         []string{"bytes"},
			SideEffectClass: protocol.ClassPure,
		},
	},
	"time.Time": {
		{
			Expression:      "time.Time{}",
			TypeHint:        "time.Time",
			Imports:         []string{"time"},
			SideEffectClass: protocol.ClassPure,
		},
		{
			Expression:      "time.Now()",
			TypeHint:        "time.Time",
			Imports:         []string{"time"},
			SideEffectClass: protocol.ClassPure,
		},
	},
	"http.Header": {
		{
			Expression:      "http.Header{}",
			TypeHint:        "http.Header",
			Imports:         []string{"net/http"},
			SideEffectClass: protocol.ClassPure,
		},
	},
}

// LookupRuntimeValue returns the ordered runtime-value candidates registered
// for the given Go type spelling. The returned slice is a copy; callers may
// mutate it freely. Imports on each entry are sorted and deduplicated.
//
// Unknown type spellings return nil. The lookup is case-sensitive and
// matches the exact spelling; it does not strip aliases or interface
// wrappers.
func LookupRuntimeValue(typeName string) []RuntimeValue {
	entries, ok := runtimeValueRegistry[typeName]
	if !ok {
		return nil
	}
	out := make([]RuntimeValue, len(entries))
	for i, e := range entries {
		out[i] = e
		out[i].Imports = sortedUniqueImports(e.Imports)
		if out[i].SideEffectClass == "" {
			out[i].SideEffectClass = protocol.ClassPure
		}
	}
	return out
}

// RegisteredRuntimeValueTypes returns the sorted list of type spellings the
// registry currently recognizes. Intended for diagnostics and tests.
func RegisteredRuntimeValueTypes() []string {
	out := make([]string, 0, len(runtimeValueRegistry))
	for k := range runtimeValueRegistry {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
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

func sortedUniqueImports(paths []string) []string {
	if len(paths) == 0 {
		return nil
	}
	seen := make(map[string]struct{}, len(paths))
	for _, p := range paths {
		if p == "" {
			continue
		}
		seen[p] = struct{}{}
	}
	if len(seen) == 0 {
		return nil
	}
	out := make([]string, 0, len(seen))
	for p := range seen {
		out = append(out, p)
	}
	sort.Strings(out)
	return out
}
