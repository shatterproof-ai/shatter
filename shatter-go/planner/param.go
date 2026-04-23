package planner

import (
	"encoding/json"
	"fmt"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// DefaultMaxParamValuePlans caps the number of ValuePlans produced for a
// single parameter when ParamPlanOptions.MaxPlansPerParam is zero.
const DefaultMaxParamValuePlans = 4

// paramTypeHint is the Go type string emitted on ValuePlan.TypeHint for a
// given primitive family.
const (
	paramTypeHintString    = "string"
	paramTypeHintInt       = "int"
	paramTypeHintFloat64   = "float64"
	paramTypeHintBool      = "bool"
	paramTypeHintByteSlice = "[]byte"
)

// ParamValueHint is an operator-supplied override for a single parameter.
// When present, it becomes the top-priority ValuePlan for that parameter.
type ParamValueHint struct {
	// Literal is the raw JSON literal to emit on the ValuePlan.
	Literal json.RawMessage
	// TypeHint is the Go type string used for code generation.
	TypeHint string
}

// ParamPlanOptions bundles caller inputs for parameter-value planning.
type ParamPlanOptions struct {
	// HintsByName supplies operator overrides keyed by parameter name.
	// An entry matching a parameter produces the top-priority ValuePlan for
	// that parameter (literal kind).
	HintsByName map[string]ParamValueHint
	// MaxPlansPerParam caps each parameter's ValuePlan slice. Zero means
	// DefaultMaxParamValuePlans.
	MaxPlansPerParam int
}

// PlanParams plans values for every parameter in params, returning a matrix
// of candidate ValuePlans (one slice per parameter, in declaration order)
// plus any UnsatisfiedRequirements for parameters the planner cannot handle.
//
// Per the E4 contract, an unsupported parameter does not block the whole
// plan: the matrix slot for that parameter is nil and an
// UnsatisfiedRequirement is appended to the returned slice, but sibling
// parameters are still planned.
func PlanParams(targetID string, params []protocol.ParamInfo, opts ParamPlanOptions) ([][]protocol.ValuePlan, []protocol.UnsatisfiedRequirement) {
	if len(params) == 0 {
		return nil, nil
	}
	matrix := make([][]protocol.ValuePlan, len(params))
	var unsatisfied []protocol.UnsatisfiedRequirement
	for i, p := range params {
		plans, u := PlanParam(targetID, i, p, opts)
		if u != nil {
			unsatisfied = append(unsatisfied, *u)
			continue
		}
		matrix[i] = plans
	}
	return matrix, unsatisfied
}

// PlanParam plans values for a single parameter at paramIndex, returning
// either a prioritised slice of ValuePlans or an UnsatisfiedRequirement
// describing why the parameter cannot be planned.
//
// Primitive families supported: string, int, float, bool, []byte. Any other
// TypeInfo.Kind (opaque, object, nullable, complex, unknown, array-of-non-
// byte) yields UnsatisfiedRequirementKindComplexType.
func PlanParam(targetID string, paramIndex int, p protocol.ParamInfo, opts ParamPlanOptions) ([]protocol.ValuePlan, *protocol.UnsatisfiedRequirement) {
	maxPlans := opts.MaxPlansPerParam
	if maxPlans <= 0 {
		maxPlans = DefaultMaxParamValuePlans
	}

	family, ok := classifyParamFamily(p)
	if !ok {
		if aggPlans, aggUnsat := PlanAggregate(targetID, paramIndex, p, maxPlans); aggPlans != nil {
			return aggPlans, nil
		} else if aggUnsat != nil {
			return nil, aggUnsat
		}
		if runtimePlans := runtimeValuePlans(paramIndex, p, maxPlans); len(runtimePlans) > 0 {
			return runtimePlans, nil
		}
		if fbPlans, fbUnsat := PlanFallback(targetID, paramIndex, p, maxPlans); fbPlans != nil {
			return fbPlans, nil
		} else if fbUnsat != nil {
			return nil, fbUnsat
		}
		return nil, &protocol.UnsatisfiedRequirement{
			Kind:     protocol.UnsatisfiedRequirementKindComplexType,
			TargetID: targetID,
			Detail:   paramUnsupportedDetail(p),
		}
	}

	plans := make([]protocol.ValuePlan, 0, maxPlans)
	add := func(plan protocol.ValuePlan) bool {
		if len(plans) >= maxPlans {
			return false
		}
		plan.ParamIndex = paramIndex
		plan.ParamName = p.Name
		plans = append(plans, plan)
		return true
	}

	if hint, found := opts.HintsByName[p.Name]; found {
		typeHint := hint.TypeHint
		if typeHint == "" {
			typeHint = family.typeHint
		}
		add(protocol.ValuePlan{
			Kind:     protocol.ValuePlanKindLiteral,
			Literal:  hint.Literal,
			TypeHint: typeHint,
		})
	}

	for _, cand := range family.candidates {
		if !add(protocol.ValuePlan{
			Kind:     cand.kind,
			Literal:  cand.literal,
			TypeHint: family.typeHint,
		}) {
			break
		}
	}

	if len(plans) == 0 {
		// A hint with an empty candidate set and a cap of zero could land
		// here; treated as unsatisfied so the caller sees the failure.
		return nil, &protocol.UnsatisfiedRequirement{
			Kind:     protocol.UnsatisfiedRequirementKindComplexType,
			TargetID: targetID,
			Detail:   paramUnsupportedDetail(p),
		}
	}
	return plans, nil
}

// paramFamily carries the code-generation type hint plus the ordered
// default candidate list for one primitive family.
type paramFamily struct {
	typeHint   string
	candidates []paramCandidate
}

type paramCandidate struct {
	kind    protocol.ValuePlanKind
	literal json.RawMessage
}

func classifyParamFamily(p protocol.ParamInfo) (paramFamily, bool) {
	// Prefer TypeName when present — it carries the exact Go source spelling,
	// which disambiguates ambiguous TypeInfo shapes (e.g. []byte vs []int).
	if p.TypeName != nil {
		switch *p.TypeName {
		case "[]byte", "[]uint8":
			return byteSliceFamily(), true
		}
	}
	switch p.Type.Kind {
	case "str":
		return stringFamily(), true
	case "int":
		return intFamily(), true
	case "float":
		return floatFamily(), true
	case "bool":
		return boolFamily(), true
	case "array":
		// Without TypeName we can't distinguish []byte from []int; only treat
		// an array family as []byte when the TypeName explicitly says so.
		return paramFamily{}, false
	default:
		return paramFamily{}, false
	}
}

func stringFamily() paramFamily {
	return paramFamily{
		typeHint: paramTypeHintString,
		candidates: []paramCandidate{
			{kind: protocol.ValuePlanKindZero},
			{kind: protocol.ValuePlanKindLiteral, literal: json.RawMessage(`"a"`)},
			{kind: protocol.ValuePlanKindLiteral, literal: json.RawMessage(`"hello"`)},
		},
	}
}

func intFamily() paramFamily {
	return paramFamily{
		typeHint: paramTypeHintInt,
		candidates: []paramCandidate{
			{kind: protocol.ValuePlanKindZero},
			{kind: protocol.ValuePlanKindLiteral, literal: json.RawMessage(`1`)},
			{kind: protocol.ValuePlanKindLiteral, literal: json.RawMessage(`-1`)},
		},
	}
}

func floatFamily() paramFamily {
	return paramFamily{
		typeHint: paramTypeHintFloat64,
		candidates: []paramCandidate{
			{kind: protocol.ValuePlanKindZero},
			{kind: protocol.ValuePlanKindLiteral, literal: json.RawMessage(`1.5`)},
			{kind: protocol.ValuePlanKindLiteral, literal: json.RawMessage(`-1.5`)},
		},
	}
}

func boolFamily() paramFamily {
	return paramFamily{
		typeHint: paramTypeHintBool,
		candidates: []paramCandidate{
			{kind: protocol.ValuePlanKindZero},
			{kind: protocol.ValuePlanKindLiteral, literal: json.RawMessage(`true`)},
		},
	}
}

func byteSliceFamily() paramFamily {
	// Go's json.Marshal encodes []byte as a base64 string, so the literal
	// slot carries pre-encoded base64 values the wrapper decodes at runtime.
	return paramFamily{
		typeHint: paramTypeHintByteSlice,
		candidates: []paramCandidate{
			{kind: protocol.ValuePlanKindZero},
			{kind: protocol.ValuePlanKindLiteral, literal: json.RawMessage(`"aGVsbG8="`)}, // "hello"
			{kind: protocol.ValuePlanKindLiteral, literal: json.RawMessage(`""`)},         // empty slice
		},
	}
}

func paramUnsupportedDetail(p protocol.ParamInfo) string {
	label := paramTypeLabel(p)
	if p.Name == "" {
		return fmt.Sprintf("parameter of type %s is not a supported primitive", label)
	}
	return fmt.Sprintf("parameter %q of type %s is not a supported primitive", p.Name, label)
}

func paramTypeLabel(p protocol.ParamInfo) string {
	if p.TypeName != nil && *p.TypeName != "" {
		return *p.TypeName
	}
	if p.Type.Label != "" {
		return p.Type.Label
	}
	if p.Type.ComplexKind != "" {
		return p.Type.ComplexKind
	}
	if p.Type.Kind != "" {
		return p.Type.Kind
	}
	return "unknown"
}
