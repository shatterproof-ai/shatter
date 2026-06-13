package planner

import (
	"encoding/json"
	"sort"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// DefaultMaxComposedPlansPerTarget caps the number of composed
// InvocationPlans returned per target when ComposeOptions.MaxPlans is zero.
const DefaultMaxComposedPlansPerTarget = 5

const constructorExpressionParamJSONLiteral = `"true"`

// ComposeOptions bundles caller-supplied knobs for Compose.
type ComposeOptions struct {
	// MaxPlans caps the final composed plan count. Zero means
	// DefaultMaxComposedPlansPerTarget.
	MaxPlans int
	// BeamWidth caps per-step intermediate state during the parameter walk.
	// Zero means MaxPlans (after defaulting), which is sufficient for the
	// final Top-K cap when the parameter axis is small.
	BeamWidth int
	// IsMethod signals that the target is a method and that an empty
	// receiverPlans slice should produce an UnsatisfiedRequirement rather
	// than a single no-receiver plan. For free functions IsMethod must be
	// false and receiverPlans must be nil.
	IsMethod bool
}

// Compose produces ranked InvocationPlans by combining receiver plans with
// per-parameter ValuePlans. It performs a deterministic beam search over the
// parameter axis and returns the Top-K plans by the ranking tuple
// (hintDepCount asc, receiverPriority asc, enumerationIndex asc).
//
// When paramUnsatisfied is non-empty or paramMatrix contains a nil slot,
// Compose returns no plans and propagates the aggregated unsatisfied
// requirements. When opts.IsMethod is true and receiverPlans is empty, it
// emits an UnsatisfiedRequirementKindNoConstructor for targetID.
//
// For free functions pass receiverPlans=nil and opts.IsMethod=false; each
// composed plan carries ReceiverKind="".
func Compose(
	targetID string,
	receiverPlans []ReceiverPlan,
	paramMatrix [][]protocol.ValuePlan,
	paramUnsatisfied []protocol.UnsatisfiedRequirement,
	opts ComposeOptions,
) ([]protocol.InvocationPlan, []protocol.UnsatisfiedRequirement) {
	maxPlans := opts.MaxPlans
	if maxPlans <= 0 {
		maxPlans = DefaultMaxComposedPlansPerTarget
	}
	beamWidth := opts.BeamWidth
	if beamWidth <= 0 {
		beamWidth = maxPlans
	}

	if len(paramUnsatisfied) > 0 {
		return nil, paramUnsatisfied
	}
	for i, slot := range paramMatrix {
		if slot == nil {
			return nil, []protocol.UnsatisfiedRequirement{{
				Kind:     protocol.UnsatisfiedRequirementKindComplexType,
				TargetID: targetID,
				Detail:   missingParamDetail(i),
			}}
		}
	}

	if opts.IsMethod && len(receiverPlans) == 0 {
		return nil, []protocol.UnsatisfiedRequirement{{
			Kind:     protocol.UnsatisfiedRequirementKindNoConstructor,
			TargetID: targetID,
			Detail:   "method target has no receiver plan",
		}}
	}

	effectiveReceivers := receiverPlans
	freeFunction := false
	if len(effectiveReceivers) == 0 {
		effectiveReceivers = []ReceiverPlan{{
			Kind:         "",
			ReceiverKind: "",
			Label:        "free_function",
			Priority:     0,
		}}
		freeFunction = true
	}

	candidates := enumerateCandidates(targetID, effectiveReceivers, paramMatrix, freeFunction, beamWidth)

	sort.SliceStable(candidates, func(i, j int) bool {
		return candidates[i].less(candidates[j])
	})

	if len(candidates) > maxPlans {
		candidates = candidates[:maxPlans]
	}

	plans := make([]protocol.InvocationPlan, len(candidates))
	for i, c := range candidates {
		plan := c.plan
		plan.Priority = i
		plans[i] = plan
	}
	return plans, nil
}

// composeCandidate carries the ranking-relevant metadata for one beam entry.
type composeCandidate struct {
	plan             protocol.InvocationPlan
	hintDepCount     int
	receiverPriority int
	enumerationIndex int
}

func (c composeCandidate) less(other composeCandidate) bool {
	if c.hintDepCount != other.hintDepCount {
		return c.hintDepCount < other.hintDepCount
	}
	if c.receiverPriority != other.receiverPriority {
		return c.receiverPriority < other.receiverPriority
	}
	return c.enumerationIndex < other.enumerationIndex
}

// enumerateCandidates walks the cartesian product of receiver × parameter
// slots under a beam-width cap. The cap bounds intermediate-state growth
// when the parameter axis is wide; smaller beams favour the lowest-scored
// partials at each step.
func enumerateCandidates(
	targetID string,
	receiverPlans []ReceiverPlan,
	paramMatrix [][]protocol.ValuePlan,
	freeFunction bool,
	beamWidth int,
) []composeCandidate {
	candidates := make([]composeCandidate, 0, len(receiverPlans)*maxInt(1, productOfLengths(paramMatrix)))
	enumeration := 0
	for _, recv := range receiverPlans {
		partials := []composeCandidate{{
			plan: protocol.InvocationPlan{
				TargetID:            targetID,
				ReceiverKind:        recv.ReceiverKind,
				ArgumentPlans:       []protocol.ValuePlan{},
				ConstructorArgPlans: constructorArgPlansForReceiver(recv),
				Label:               labelForReceiver(recv, freeFunction),
			},
			hintDepCount:     receiverHintDep(recv),
			receiverPriority: recv.Priority,
			enumerationIndex: enumeration,
		}}
		enumeration++

		for _, slot := range paramMatrix {
			next := make([]composeCandidate, 0, len(partials)*len(slot))
			for _, partial := range partials {
				for _, vp := range slot {
					extended := partial
					extended.plan.ArgumentPlans = append(append([]protocol.ValuePlan{}, partial.plan.ArgumentPlans...), vp)
					extended.enumerationIndex = enumeration
					enumeration++
					next = append(next, extended)
				}
			}
			sort.SliceStable(next, func(i, j int) bool {
				return next[i].less(next[j])
			})
			if len(next) > beamWidth {
				next = next[:beamWidth]
			}
			partials = next
		}
		candidates = append(candidates, partials...)
	}
	return candidates
}

func receiverHintDep(r ReceiverPlan) int {
	if r.Kind == ReceiverPlanKindHint {
		return 1
	}
	return 0
}

func labelForReceiver(r ReceiverPlan, freeFunction bool) string {
	if freeFunction {
		return r.Label
	}
	if r.Label != "" {
		return r.Label
	}
	return string(r.Kind)
}

func productOfLengths(matrix [][]protocol.ValuePlan) int {
	p := 1
	for _, slot := range matrix {
		p *= len(slot)
	}
	return p
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func missingParamDetail(paramIndex int) string {
	// Callers that pass an explicit paramUnsatisfied slice get their detail
	// text preserved verbatim. This branch only fires when the caller passes
	// a nil slot without a matching UnsatisfiedRequirement — a misuse — so
	// the detail prioritises debuggability over polish.
	return "parameter at index " + itoa(paramIndex) + " has no ValuePlan"
}

func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	neg := n < 0
	if neg {
		n = -n
	}
	var buf [20]byte
	i := len(buf)
	for n > 0 {
		i--
		buf[i] = byte('0' + n%10)
		n /= 10
	}
	if neg {
		i--
		buf[i] = '-'
	}
	return string(buf[i:])
}

// constructorArgPlansForReceiver generates ValuePlans for parameterized
// constructor arguments that must come from the JSON input prefix (str-9b1q).
// Constructor parameters satisfied by the runtime-value registry are omitted:
// the generated wrapper initializes those directly and they do not consume an
// input slot. Aggregate constructor parameters consume one prefix slot as a
// JSON zero value; the Go wrapper unmarshals that into the declared aggregate
// type before calling the constructor.
func constructorArgPlansForReceiver(recv ReceiverPlan) []protocol.ValuePlan {
	if len(recv.ConstructorParams) == 0 {
		return nil
	}
	plans := make([]protocol.ValuePlan, 0, len(recv.ConstructorParams))
	for i, p := range recv.ConstructorParams {
		if len(runtimeValuePlans(i, p, 1)) > 0 {
			continue
		}
		if constructorRuntimeValueParamSatisfiable(p, recv.ConstructorRuntimeValuesByParam) {
			continue
		}
		if interfaceImplConstructorParamSatisfiable(p, recv.ConstructorInterfaceImplsByParam) {
			continue
		}
		if constructorZeroValueParamSatisfiable(p) {
			continue
		}
		if constructorAggregateParamSatisfiable(p) {
			plans = append(plans, protocol.ValuePlan{
				Kind:       protocol.ValuePlanKindZero,
				ParamIndex: i,
				ParamName:  p.Name,
				TypeHint:   typeHintForParam(p),
			})
			continue
		}
		typeHint := typeHintForParam(p)
		if literal, ok := constructorExpressionLiteralSeedForParam(p, typeHint); ok {
			plans = append(plans, protocol.ValuePlan{
				Kind:       protocol.ValuePlanKindLiteral,
				ParamIndex: i,
				ParamName:  p.Name,
				Literal:    literal,
				TypeHint:   typeHint,
			})
			continue
		}
		plans = append(plans, protocol.ValuePlan{
			Kind:       protocol.ValuePlanKindZero,
			ParamIndex: i,
			ParamName:  p.Name,
			TypeHint:   typeHint,
		})
	}
	return plans
}

func constructorExpressionLiteralSeedForParam(p protocol.ParamInfo, typeHint string) (json.RawMessage, bool) {
	if p.Type.Kind != "str" && typeHint != paramTypeHintString {
		return nil, false
	}
	switch strings.ToLower(p.Name) {
	case "expr", "expression", "condition", "predicate":
		return json.RawMessage(constructorExpressionParamJSONLiteral), true
	default:
		return nil, false
	}
}

func constructorAggregateParamSatisfiable(p protocol.ParamInfo) bool {
	plans, unsat := planAggregateWithOptions("", 0, p, 1, aggregateOptions{})
	return len(plans) > 0 && unsat == nil
}

// typeHintForParam returns the Go type hint string for a ParamInfo (str-9b1q).
func typeHintForParam(p protocol.ParamInfo) string {
	if p.TypeName != nil {
		return *p.TypeName
	}
	switch p.Type.Kind {
	case "str":
		return "string"
	case "int":
		return "int"
	case "float":
		return "float64"
	case "bool":
		return "bool"
	default:
		return "string"
	}
}
