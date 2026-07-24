package planner

import (
	"encoding/json"
	"fmt"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// nilExpression is the Go source expression for the zero value of error,
// chan, and func parameter types.
const nilExpression = "nil"

// errorLiteralExpression is the non-nil error ValuePlan candidate.
const errorLiteralExpression = `fmt.Errorf("err")`

// errorTypeHint is the Go source spelling of the error type.
const errorTypeHint = "error"

// chanTypeNamePrefix is the leading substring a channel parameter's TypeName
// carries (matches directional variants `chan<- ` and `<-chan ` via the Trim
// below).
const chanTypeNamePrefix = "chan "

// chanSendOnlyPrefix and chanReceiveOnlyPrefix are the directional channel
// type prefixes. `make(chan<- T)` and `make(<-chan T)` are not legal Go, so
// directional channel params only receive a nil ValuePlan.
const (
	chanSendOnlyPrefix    = "chan<- "
	chanReceiveOnlyPrefix = "<-chan "
)

// funcTypeNamePrefix is the leading substring a function-typed parameter's
// TypeName carries.
const funcTypeNamePrefix = "func"

// PlanFallback synthesizes ValuePlans for error, chan, and func parameter
// types that the primitive / aggregate / runtime-value paths do not cover
// (str-hy9b.F5).
//
// The returned triple has three states:
//   - (nonempty plans, nil): the parameter shape was recognized and planned.
//   - (nil, &unsatisfied): the shape matched but could not be synthesized
//     (e.g. channel with no element spelling).
//   - (nil, nil): p is not an error/chan/func parameter; the caller should
//     fall through to its next strategy.
//
// maxPlans caps the returned slice. maxPlans <= 0 is treated as 1.
func PlanFallback(targetID string, paramIndex int, p protocol.ParamInfo, maxPlans int) ([]protocol.ValuePlan, *protocol.UnsatisfiedRequirement) {
	if maxPlans <= 0 {
		maxPlans = 1
	}
	switch classifyFallback(p) {
	case fallbackError:
		return planErrorFallback(paramIndex, p, maxPlans), nil
	case fallbackChan:
		return planChanFallback(targetID, paramIndex, p, maxPlans)
	case fallbackFunc:
		return planFuncFallback(paramIndex, p, maxPlans), nil
	default:
		return nil, nil
	}
}

// isErrorParam reports whether p is a bare builtin `error` parameter. It mirrors
// classifyFallback's error detection but is consulted from PlanParam before the
// primitive/aggregate/fallback cascade so error params take the tagged-error
// Literal planning path (str-kvzh7) instead of the runtime_value fallback (which
// the core materializes as null).
func isErrorParam(p protocol.ParamInfo) bool {
	if fallbackTypeName(p) == errorTypeHint {
		return true
	}
	return p.Type.Kind == "complex" && p.Type.ComplexKind == "error"
}

// planErrorParam builds the Literal ValuePlan slice for a bare `error`
// parameter (str-kvzh7). Candidates, in priority order:
//
//  1. nil (JSON null) — the error-absent branch; also covers default arms.
//  2. one `{"__complex_type":"error","sentinel":N}` selector per mined
//     errors.Is/errors.As sentinel target, which the wrapper's baked sentinel
//     table resolves to the actual sentinel variable so `errors.Is(err, pkg.ErrX)`
//     branches become reachable.
//  3. errors.New("shatter") via the tagged `{"__complex_type":"error",...}`
//     shape — a generic non-nil error for `err != nil` arms.
//
// Sentinels rank ahead of the generic errors.New candidate: they are exact
// known-answer values, whereas the default/`err != nil` arm the errors.New
// candidate covers is also reachable via nil, so errors.New is the expendable
// one under the invocation-plan-level cap.
//
// sentinelCount is the number of sentinels mined for this param (wrapper table
// length); indices [0, sentinelCount) align with that table. The per-parameter
// cap is raised to fit every candidate because sentinels are exact known-answer
// values that must not be evicted; the invocation-plan-level cap still bounds
// the emitted plan-set size.
func planErrorParam(paramIndex int, p protocol.ParamInfo, sentinelCount, maxPlans int) []protocol.ValuePlan {
	if sentinelCount < 0 {
		sentinelCount = 0
	}
	needed := 2 + sentinelCount
	if maxPlans < needed {
		maxPlans = needed
	}
	plans := make([]protocol.ValuePlan, 0, needed)
	add := func(literal json.RawMessage) {
		if len(plans) >= maxPlans {
			return
		}
		plans = append(plans, protocol.ValuePlan{
			ParamIndex: paramIndex,
			ParamName:  p.Name,
			Kind:       protocol.ValuePlanKindLiteral,
			Literal:    literal,
			TypeHint:   errorTypeHint,
		})
	}
	add(json.RawMessage("null"))
	for n := range sentinelCount {
		add(json.RawMessage(fmt.Sprintf(`{"__complex_type":"error","sentinel":%d}`, n)))
	}
	add(json.RawMessage(`{"__complex_type":"error","message":"shatter"}`))
	return plans
}

type fallbackKind int

const (
	fallbackNone fallbackKind = iota
	fallbackError
	fallbackChan
	fallbackFunc
)

func classifyFallback(p protocol.ParamInfo) fallbackKind {
	typeName := fallbackTypeName(p)

	if typeName == errorTypeHint {
		return fallbackError
	}
	if p.Type.Kind == "complex" && p.Type.ComplexKind == "error" {
		return fallbackError
	}

	switch {
	case strings.HasPrefix(typeName, chanTypeNamePrefix),
		strings.HasPrefix(typeName, chanSendOnlyPrefix),
		strings.HasPrefix(typeName, chanReceiveOnlyPrefix):
		return fallbackChan
	}
	if p.Type.Kind == "opaque" && strings.HasPrefix(p.Type.Label, chanTypeNamePrefix) {
		return fallbackChan
	}

	if strings.HasPrefix(typeName, funcTypeNamePrefix) {
		return fallbackFunc
	}

	return fallbackNone
}

func fallbackTypeName(p protocol.ParamInfo) string {
	if p.TypeName == nil {
		return ""
	}
	return strings.TrimSpace(*p.TypeName)
}

// planErrorFallback emits nil and fmt.Errorf("err") ValuePlans.
func planErrorFallback(paramIndex int, p protocol.ParamInfo, maxPlans int) []protocol.ValuePlan {
	expressions := []string{nilExpression, errorLiteralExpression}
	return fallbackValuePlans(paramIndex, p.Name, errorTypeHint, expressions, maxPlans)
}

// planChanFallback emits nil and (for non-directional channels) a make(chan T)
// ValuePlan. A channel with no spellable TypeName — or a directional channel
// where `make` is not legal — falls back to nil only; a channel TypeInfo with
// no TypeName at all is unsatisfiable because the make expression needs the
// full type spelling.
func planChanFallback(targetID string, paramIndex int, p protocol.ParamInfo, maxPlans int) ([]protocol.ValuePlan, *protocol.UnsatisfiedRequirement) {
	typeName := fallbackTypeName(p)
	if typeName == "" {
		// Type.Kind=="opaque" + Label "chan X" can land here. The ValuePlan
		// needs the Go source spelling, which Label does not formally promise.
		return nil, &protocol.UnsatisfiedRequirement{
			Kind:     protocol.UnsatisfiedRequirementKindComplexType,
			TargetID: targetID,
			Detail:   paramUnsupportedDetail(p),
		}
	}
	expressions := []string{nilExpression}
	// Directional channels (`chan<- T`, `<-chan T`) can't be constructed with
	// `make` using the directional spelling; `make(chan T)` would widen the
	// type and fail to satisfy the parameter. Keep nil only.
	if strings.HasPrefix(typeName, chanTypeNamePrefix) {
		expressions = append(expressions, "make("+typeName+")")
	}
	return fallbackValuePlans(paramIndex, p.Name, typeName, expressions, maxPlans), nil
}

// planFuncFallback emits a single nil ValuePlan for any top-level func-typed
// parameter. Non-nil function-literal synthesis is deferred.
func planFuncFallback(paramIndex int, p protocol.ParamInfo, maxPlans int) []protocol.ValuePlan {
	typeName := fallbackTypeName(p)
	if typeName == "" {
		typeName = funcTypeNamePrefix
	}
	return fallbackValuePlans(paramIndex, p.Name, typeName, []string{nilExpression}, maxPlans)
}

// fallbackValuePlans wraps Go source expressions as runtime-value ValuePlans,
// mirroring aggregate.aggregateValuePlans so the wrapper generator sees a
// uniform kind.
func fallbackValuePlans(paramIndex int, paramName, typeHint string, expressions []string, maxPlans int) []protocol.ValuePlan {
	if maxPlans > 0 && len(expressions) > maxPlans {
		expressions = expressions[:maxPlans]
	}
	plans := make([]protocol.ValuePlan, 0, len(expressions))
	for _, expr := range expressions {
		literal, err := json.Marshal(expr)
		if err != nil {
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
