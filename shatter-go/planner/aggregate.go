package planner

import (
	"encoding/json"
	"fmt"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// aggregateKind classifies a parameter's aggregate-type shape.
type aggregateKind int

const (
	aggregateNone aggregateKind = iota
	aggregateSlice
	aggregateMap
	aggregateStruct
)

// Pseudo-field names the analyzer uses to encode a Go map type as an
// "object" TypeInfo (see mapTypeInfoRec in shatter-go/protocol/analyzer.go).
// An object TypeInfo with exactly these two field names is a map encoding;
// any other object shape is a struct.
const (
	mapPseudoFieldKey   = "_key"
	mapPseudoFieldValue = "_value"
)

// mapTypeNamePrefix is the leading substring a Go map's TypeName carries.
const mapTypeNamePrefix = "map["

// PlanAggregate synthesizes ValuePlans for slice, map, and struct parameters.
//
// The returned triple has three states:
//   - (nonempty plans, nil): the aggregate was synthesized.
//   - (nil, &unsatisfied): the parameter matched an aggregate shape but the
//     element / key / value / field type could not be synthesized.
//   - (nil, nil): p is not an aggregate parameter; the caller should fall
//     through to its next strategy (e.g. the runtime-value registry).
//
// maxPlans caps the returned slice. maxPlans <= 0 is treated as 1 so that
// at least one plan is returned when synthesis succeeds.
func PlanAggregate(targetID string, paramIndex int, p protocol.ParamInfo, maxPlans int) ([]protocol.ValuePlan, *protocol.UnsatisfiedRequirement) {
	if maxPlans <= 0 {
		maxPlans = 1
	}
	switch classifyAggregate(p) {
	case aggregateSlice:
		return planSliceAggregate(targetID, paramIndex, p, maxPlans)
	case aggregateMap:
		return planMapAggregate(targetID, paramIndex, p, maxPlans)
	case aggregateStruct:
		return planStructAggregate(targetID, paramIndex, p, maxPlans)
	default:
		return nil, nil
	}
}

// classifyAggregate returns the aggregate kind p maps to, or aggregateNone
// when p is not an aggregate. The slice and map cases require a non-empty
// TypeName because expression synthesis must spell the full Go type.
func classifyAggregate(p protocol.ParamInfo) aggregateKind {
	typeName := ""
	if p.TypeName != nil {
		typeName = strings.TrimSpace(*p.TypeName)
	}
	switch p.Type.Kind {
	case "array":
		if typeName == "" {
			return aggregateNone
		}
		// Byte slices are handled by the primitive family; classify them out
		// so the aggregate planner does not shadow the existing behavior.
		if typeName == "[]byte" || typeName == "[]uint8" {
			return aggregateNone
		}
		return aggregateSlice
	case "object":
		if isMapTypeInfo(p.Type) || strings.HasPrefix(typeName, mapTypeNamePrefix) {
			if typeName == "" {
				return aggregateNone
			}
			return aggregateMap
		}
		if typeName == "" {
			return aggregateNone
		}
		return aggregateStruct
	default:
		return aggregateNone
	}
}

// isMapTypeInfo reports whether t is the analyzer's map encoding: an object
// with exactly two fields named "_key" and "_value".
func isMapTypeInfo(t protocol.TypeInfo) bool {
	if t.Kind != "object" || len(t.Fields) != 2 {
		return false
	}
	return t.Fields[0].Name == mapPseudoFieldKey && t.Fields[1].Name == mapPseudoFieldValue
}

// planSliceAggregate emits the zero-length and one-element ValuePlans for a
// slice parameter. An unsupported element type produces an unsatisfied.
func planSliceAggregate(targetID string, paramIndex int, p protocol.ParamInfo, maxPlans int) ([]protocol.ValuePlan, *protocol.UnsatisfiedRequirement) {
	typeName := *p.TypeName
	if p.Type.Element == nil {
		return nil, unsatisfiedAggregate(targetID, p.Name, typeName, "slice TypeInfo missing element")
	}
	elemZero, err := synthesizeFieldValue(*p.Type.Element, DefaultMaxCompositeDepth)
	if err != nil {
		return nil, unsatisfiedAggregate(targetID, p.Name, typeName, err.Error())
	}
	expressions := []string{
		typeName + "{}",
		typeName + "{" + elemZero + "}",
	}
	return aggregateValuePlans(paramIndex, p.Name, typeName, expressions, maxPlans), nil
}

// planMapAggregate emits the empty-map and one-entry ValuePlans for a map
// parameter. Unsupported key or value types produce an unsatisfied.
func planMapAggregate(targetID string, paramIndex int, p protocol.ParamInfo, maxPlans int) ([]protocol.ValuePlan, *protocol.UnsatisfiedRequirement) {
	typeName := *p.TypeName
	keyType, valueType, ok := mapKeyValueTypes(p.Type)
	if !ok {
		return nil, unsatisfiedAggregate(targetID, p.Name, typeName, "map TypeInfo missing key or value")
	}
	keyZero, err := synthesizeFieldValue(keyType, DefaultMaxCompositeDepth)
	if err != nil {
		return nil, unsatisfiedAggregate(targetID, p.Name, typeName, fmt.Sprintf("map key: %s", err.Error()))
	}
	valueZero, err := synthesizeFieldValue(valueType, DefaultMaxCompositeDepth)
	if err != nil {
		return nil, unsatisfiedAggregate(targetID, p.Name, typeName, fmt.Sprintf("map value: %s", err.Error()))
	}
	expressions := []string{
		typeName + "{}",
		typeName + "{" + keyZero + ": " + valueZero + "}",
	}
	return aggregateValuePlans(paramIndex, p.Name, typeName, expressions, maxPlans), nil
}

// planStructAggregate emits a single ValuePlan for a struct parameter via
// the existing composite-literal synthesizer. The pkgImport metadata is not
// carried on the ValuePlan today; callers that need it consult PlanComposite
// directly.
func planStructAggregate(targetID string, paramIndex int, p protocol.ParamInfo, maxPlans int) ([]protocol.ValuePlan, *protocol.UnsatisfiedRequirement) {
	typeName := *p.TypeName
	composite, unsat := PlanComposite(targetID, typeName, "", p.Type, CompositeOptions{})
	if unsat != nil {
		return nil, unsat
	}
	expressions := []string{composite.Expression}
	return aggregateValuePlans(paramIndex, p.Name, typeName, expressions, maxPlans), nil
}

// mapKeyValueTypes extracts the map's key and value TypeInfo from the
// analyzer's pseudo-field encoding.
func mapKeyValueTypes(t protocol.TypeInfo) (protocol.TypeInfo, protocol.TypeInfo, bool) {
	if !isMapTypeInfo(t) {
		return protocol.TypeInfo{}, protocol.TypeInfo{}, false
	}
	return t.Fields[0].Type, t.Fields[1].Type, true
}

// aggregateValuePlans wraps a list of Go source expressions as
// runtime-value ValuePlans. The literal slot carries the JSON-encoded
// expression string so downstream code that already handles the
// runtime-value kind (F2) can emit the expression verbatim.
func aggregateValuePlans(paramIndex int, paramName, typeHint string, expressions []string, maxPlans int) []protocol.ValuePlan {
	if maxPlans > 0 && len(expressions) > maxPlans {
		expressions = expressions[:maxPlans]
	}
	plans := make([]protocol.ValuePlan, 0, len(expressions))
	for _, expr := range expressions {
		literal, err := json.Marshal(expr)
		if err != nil {
			// json.Marshal on a valid UTF-8 string cannot fail; skip
			// defensively rather than crash the planner.
			continue
		}
		plans = append(plans, protocol.ValuePlan{
			ParamIndex: paramIndex,
			ParamName:  paramName,
			Kind:       protocol.ValuePlanKindRuntimeValue,
			Literal:    literal,
			TypeHint:   typeHint,
		})
	}
	return plans
}

func unsatisfiedAggregate(targetID, paramName, typeName, cause string) *protocol.UnsatisfiedRequirement {
	var detail string
	switch {
	case paramName != "" && typeName != "":
		detail = fmt.Sprintf("parameter %q of type %s: %s", paramName, typeName, cause)
	case paramName != "":
		detail = fmt.Sprintf("parameter %q: %s", paramName, cause)
	case typeName != "":
		detail = fmt.Sprintf("parameter of type %s: %s", typeName, cause)
	default:
		detail = cause
	}
	return &protocol.UnsatisfiedRequirement{
		Kind:     protocol.UnsatisfiedRequirementKindComplexType,
		TargetID: targetID,
		Detail:   detail,
	}
}
