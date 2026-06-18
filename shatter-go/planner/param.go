package planner

import (
	"encoding/json"
	"fmt"

	"github.com/shatter-dev/shatter/shatter-go/config"
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
	paramTypeHintUint      = "uint64"
	paramTypeHintFloat64   = "float64"
	paramTypeHintBool      = "bool"
	paramTypeHintByteSlice = "[]byte"
	paramTypeHintDuration  = "time.Duration"
)

// Nanosecond literals used by durationFamily. time.Duration is an int64 alias
// in nanoseconds; emitting these integer literals directly lets the wrapper's
// json.Unmarshal consume them into the parameter without conversion.
const (
	durationOneMillisecondNanos = int64(1_000_000)
	durationOneSecondNanos      = int64(1_000_000_000)
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
	// GeneratorsByName names a runtime-value registry entry per parameter
	// (parameter name → registered Go type spelling, e.g. "context.Context").
	// When a parameter has a generator entry, PlanParam consults the named
	// generator before falling back to primitive families. This is the
	// hint_config_v1 generators surface (str-hy9b.G3 AC3).
	GeneratorsByName map[string]string
	// ConfiguredRuntimeValues supplies exact type-spelling runtime values from
	// the project .shatter/config.yaml go_runtime_values section.
	ConfiguredRuntimeValues map[string]config.GoRuntimeValueConfig
	// InterfaceImplsByParam maps parameter names to discovered interface
	// implementation candidates (str-4v9h). When a parameter is typed as an
	// imported interface, the handler discovers constructors from the
	// interface's defining package and passes them here. PlanParam routes
	// these through PlanInterfaceImpls.
	InterfaceImplsByParam map[string][]protocol.InterfaceParamCandidate
	// JSONEncodeInterfaceParams marks empty-interface parameters that flow
	// directly into encoding/json encode APIs. Those parameters get a bounded
	// deterministic JSON-serializable candidate family instead of remaining
	// opaque. Decode-style destinations are intentionally not included.
	JSONEncodeInterfaceParams map[string]bool
	// StringLiteralsByParam maps parameter names to string literals harvested
	// from target-local control flow. Matching string parameters receive these
	// candidates before generic primitive-family defaults.
	StringLiteralsByParam map[string][]string
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

	// AC3 (str-hy9b.G3): a named generator on this parameter takes priority
	// over primitive-family classification. The generator name is a
	// runtime-value registry key (e.g. "context.Context"). A name with no
	// registry match is treated as a planning failure rather than silently
	// falling through, so configuration typos surface as
	// UnsatisfiedRequirementKindComplexType.
	if generatorName, ok := opts.GeneratorsByName[p.Name]; ok && generatorName != "" {
		plans := generatorPlans(paramIndex, p, generatorName, maxPlans)
		if len(plans) > 0 {
			return plans, nil
		}
		return nil, &protocol.UnsatisfiedRequirement{
			Kind:     protocol.UnsatisfiedRequirementKindComplexType,
			TargetID: targetID,
			Detail:   fmt.Sprintf("parameter %q: generator %q is not registered in the runtime-value registry", p.Name, generatorName),
		}
	}

	family, ok := classifyParamFamily(p)
	if !ok {
		if isEmptyInterfaceParam(p) {
			return jsonInterfaceValuePlans(paramIndex, p, maxPlans), nil
		}
		if aggPlans, aggUnsat := planAggregateWithOptions(targetID, paramIndex, p, maxPlans, aggregateOptions{
			ConfiguredRuntimeValues: opts.ConfiguredRuntimeValues,
			StringLiteralsByParam:   opts.StringLiteralsByParam,
		}); aggPlans != nil {
			return aggPlans, nil
		} else if aggUnsat != nil {
			return nil, aggUnsat
		}
		if runtimePlans := runtimeValuePlans(paramIndex, p, maxPlans); len(runtimePlans) > 0 {
			return runtimePlans, nil
		}
		if runtimePlans := configuredRuntimeValuePlans(paramIndex, p, opts.ConfiguredRuntimeValues, maxPlans); len(runtimePlans) > 0 {
			return runtimePlans, nil
		}
		if fbPlans, fbUnsat := PlanFallback(targetID, paramIndex, p, maxPlans); fbPlans != nil {
			return fbPlans, nil
		} else if fbUnsat != nil {
			return nil, fbUnsat
		}
		// str-4v9h: check for interface implementation candidates discovered
		// by the handler from the interface's defining package.
		if candidates, found := opts.InterfaceImplsByParam[p.Name]; found && len(candidates) > 0 {
			implCands := protocolToImplCandidates(candidates)
			interfaceName := paramTypeLabel(p)
			return PlanInterfaceImpls(targetID, paramIndex, p, PlanInterfaceImplOptions{
				InterfaceName: interfaceName,
				Candidates:    implCands,
				MaxImpls:      maxPlans,
			})
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

	if family.typeHint == paramTypeHintString {
		for _, literal := range opts.StringLiteralsByParam[p.Name] {
			encoded, err := json.Marshal(literal)
			if err != nil {
				continue
			}
			if !add(protocol.ValuePlan{
				Kind:     protocol.ValuePlanKindLiteral,
				Literal:  encoded,
				TypeHint: family.typeHint,
			}) {
				break
			}
		}
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
		case "int64":
			family := intFamily()
			family.typeHint = "int64"
			return family, true
		case "uint", "uint8", "uint16", "uint32", "uint64":
			// str-cfsa: unsigned integer TypeName spellings map to uintFamily so
			// generated values stay non-negative and json.Unmarshal into any
			// uint width succeeds. TypeName carries the exact source spelling
			// (e.g. "uint64") when the AST-based fallback path sets it; the
			// type-checker path sets ComplexKind instead (handled below).
			return uintFamily(), true
		case "time.Duration":
			// str-is5g: time.Duration is an int64 alias in nanoseconds. Emit
			// integer-nanosecond literals so the wrapper's json.Unmarshal
			// consumes them directly. Covers zero, positive (1ms, 1s), and
			// negative (-1s) durations.
			return durationFamily(), true
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
	case "complex":
		// str-cfsa: go_uint is emitted by the type-checker path in
		// basicTypeInfo for uint/uint16/uint32/uint64 (excluding uint8/byte
		// which go through go_byte). Map it to uintFamily so generated values
		// stay non-negative.
		if p.Type.ComplexKind == "go_uint" {
			return uintFamily(), true
		}
		// str-n66n: go_duration is emitted by mapComplexKind for
		// time.Duration. Map it to durationFamily so generated values are
		// integer nanoseconds that json.Unmarshal into time.Duration succeeds.
		if p.Type.ComplexKind == "go_duration" {
			return durationFamily(), true
		}
		return paramFamily{}, false
	case "array":
		// str-79nvf: when the element type is go_byte the type checker has already
		// identified the element as byte/uint8, so the array is definitively []byte
		// even without TypeName. This lets byte-slice defaults hints apply on the
		// type-checker analysis path that populates Element but not TypeName.
		if p.Type.Element != nil &&
			p.Type.Element.Kind == "complex" &&
			p.Type.Element.ComplexKind == "go_byte" {
			return byteSliceFamily(), true
		}
		// Without TypeName or go_byte element we can't distinguish []byte from []int.
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

// uintFamily returns the paramFamily for Go unsigned integer types.
// str-cfsa: candidates are non-negative only so json.Unmarshal into any
// uint width (uint, uint8, uint16, uint32, uint64) succeeds.
func uintFamily() paramFamily {
	return paramFamily{
		typeHint: paramTypeHintUint,
		candidates: []paramCandidate{
			{kind: protocol.ValuePlanKindZero},
			{kind: protocol.ValuePlanKindLiteral, literal: json.RawMessage(`1`)},
			{kind: protocol.ValuePlanKindLiteral, literal: json.RawMessage(`255`)},
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

func durationFamily() paramFamily {
	return paramFamily{
		typeHint: paramTypeHintDuration,
		candidates: []paramCandidate{
			{kind: protocol.ValuePlanKindZero},
			{kind: protocol.ValuePlanKindLiteral, literal: json.RawMessage(fmt.Sprintf("%d", durationOneSecondNanos))},
			{kind: protocol.ValuePlanKindLiteral, literal: json.RawMessage(fmt.Sprintf("%d", durationOneMillisecondNanos))},
			{kind: protocol.ValuePlanKindLiteral, literal: json.RawMessage(fmt.Sprintf("%d", -durationOneSecondNanos))},
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

func isEmptyInterfaceParam(p protocol.ParamInfo) bool {
	label := paramTypeLabel(p)
	return (p.Type.Kind == "opaque" || p.Type.Kind == "unknown") && (label == "interface" || label == "interface{}" || label == "any")
}

func jsonInterfaceValuePlans(paramIndex int, p protocol.ParamInfo, maxPlans int) []protocol.ValuePlan {
	candidates := []json.RawMessage{
		json.RawMessage(`{"key":"value"}`),
		json.RawMessage(`["value"]`),
		json.RawMessage(`"value"`),
		json.RawMessage(`true`),
		json.RawMessage(`1.5`),
		json.RawMessage(`null`),
	}
	if maxPlans > len(candidates) {
		maxPlans = len(candidates)
	}
	plans := make([]protocol.ValuePlan, 0, maxPlans)
	for i := 0; i < maxPlans; i++ {
		plans = append(plans, protocol.ValuePlan{
			ParamIndex: paramIndex,
			ParamName:  p.Name,
			Kind:       protocol.ValuePlanKindLiteral,
			Literal:    candidates[i],
			TypeHint:   "interface{}",
		})
	}
	return plans
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

// protocolToImplCandidates converts protocol-level InterfaceParamCandidate
// values to the planner's InterfaceImplCandidate type.
func protocolToImplCandidates(pcs []protocol.InterfaceParamCandidate) []InterfaceImplCandidate {
	out := make([]InterfaceImplCandidate, len(pcs))
	for i, pc := range pcs {
		out[i] = InterfaceImplCandidate{
			TypeName:     pc.TypeName,
			SamePackage:  pc.SamePackage,
			Constructors: pc.Constructors,
		}
	}
	return out
}
