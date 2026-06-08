package planner

import (
	"fmt"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// ReceiverPlanKind classifies a receiver-construction strategy.
type ReceiverPlanKind string

const (
	// ReceiverPlanKindAdapter uses an adapter-owned receiver (e.g. an
	// httptest recorder/request pair for net/http handlers).
	ReceiverPlanKindAdapter ReceiverPlanKind = "adapter"
	// ReceiverPlanKindSamePackageConstructor constructs the receiver using a
	// constructor defined in the same package as the method.
	ReceiverPlanKindSamePackageConstructor ReceiverPlanKind = "same_package_constructor"
	// ReceiverPlanKindNearbyPackageConstructor constructs the receiver using
	// a constructor in a nearby package whose parameters are satisfiable by
	// primitives.
	ReceiverPlanKindNearbyPackageConstructor ReceiverPlanKind = "nearby_package_constructor"
	// ReceiverPlanKindCompositeLiteral builds the receiver as a composite
	// literal of a non-pointer struct with primitive fields.
	ReceiverPlanKindCompositeLiteral ReceiverPlanKind = "composite_literal"
	// ReceiverPlanKindUsefulZeroValue uses the zero value of a whitelisted
	// type whose zero state is both valid and interesting to exercise.
	ReceiverPlanKindUsefulZeroValue ReceiverPlanKind = "useful_zero_value"
	// ReceiverPlanKindFallbackZeroValue uses a zero value as a last-resort
	// fallback when no other strategy applies and the receiver type can be
	// safely zero-initialised. Pointer receivers are dispatched as
	// `&Type{}`; value receivers as the zero `Type{}` value. Emitted only
	// when the receiver is a non-interface, non-generic-unconstrained type
	// (the wrapper's `zero_value` switch case always compiles for these),
	// so callers always see at least one executable plan instead of a
	// `NoConstructor` unsatisfied requirement (str-qo1.9).
	ReceiverPlanKindFallbackZeroValue ReceiverPlanKind = "fallback_zero_value"
	// ReceiverPlanKindHint applies an operator-supplied override.
	ReceiverPlanKindHint ReceiverPlanKind = "hint"
)

// WrapperReceiverKindZeroValue is the wrapper-facing receiver token for
// zero-value construction. Mirrors shatter-go/wrapper.WrapperKindZeroValue;
// duplicated here to avoid the planner depending on the wrapper package.
const WrapperReceiverKindZeroValue = "zero_value"

// WrapperReceiverKindConstructorPrefix is prepended to a constructor
// function name to form the wrapper-facing receiver token. Mirrors
// shatter-go/wrapper.WrapperKindConstructorPrefix.
const WrapperReceiverKindConstructorPrefix = "constructor:"

// ReceiverPlan describes a single receiver-construction strategy for a
// method target.
type ReceiverPlan struct {
	// Kind names the strategy used.
	Kind ReceiverPlanKind
	// ReceiverKind is the wrapper-facing receiver token (e.g. "zero_value"
	// or "constructor:New"). Executors look up behavior by this string.
	ReceiverKind string
	// Label is a human-readable plan identifier (snake_case).
	Label string
	// Priority is the zero-based rank within the returned plan slice.
	// Lower values are tried first.
	Priority int
	// ConstructorParams holds the parameter info for parameterized
	// constructors (str-9b1q). Non-nil only when the constructor requires
	// arguments. The wrapper uses these to generate literal default values
	// in the constructor call.
	ConstructorParams []protocol.ParamInfo
	// ConstructorInterfaceImplsByParam maps constructor parameter names to
	// interface implementation candidates that the wrapper can initialize
	// directly from constructor expressions instead of JSON inputs.
	ConstructorInterfaceImplsByParam map[string][]protocol.InterfaceParamCandidate
}

// UsefulZeroValueTypes is the whitelist of receiver types whose zero value
// is usable as a method receiver. Matched against ReceiverShape.TypeName.
var UsefulZeroValueTypes = map[string]struct{}{
	"bytes.Buffer":    {},
	"bytes.Reader":    {},
	"sync.Mutex":      {},
	"sync.RWMutex":    {},
	"strings.Builder": {},
}

// DefaultMaxReceiverPlans caps the number of receiver plans returned when
// PlanOptions.MaxPlans is zero.
const DefaultMaxReceiverPlans = 3

// ReceiverHint is an operator-supplied override, typically sourced from
// .shatter/config.yaml hints.
type ReceiverHint struct {
	// ReceiverKind is the wrapper-facing token to emit (e.g. "zero_value"
	// or "constructor:NewThing").
	ReceiverKind string
	// Label is the plan label; when empty a default is generated.
	Label string
}

// PlanOptions bundles the caller-supplied context the receiver planner needs.
type PlanOptions struct {
	// Adapter, when non-nil, contributes an adapter-backed receiver plan at
	// the highest priority. Callers set this when the receiver type is
	// recognised by a registered adapter.
	Adapter *ReceiverHint
	// SamePackageConstructors lists constructor candidates whose
	// TargetType matches the receiver type and which live in the target's
	// defining package.
	SamePackageConstructors []protocol.ConstructorCandidate
	// NearbyPackageConstructors lists constructor candidates from imported
	// packages whose parameters the caller has already verified as
	// satisfiable by primitives.
	NearbyPackageConstructors []protocol.ConstructorCandidate
	// ReceiverIsCompositeLiteralSafe signals that the receiver type is a
	// non-pointer struct whose exported fields are all primitives; the
	// planner may then emit a composite-literal plan.
	ReceiverIsCompositeLiteralSafe bool
	// Hint is a caller-supplied receiver-kind override; lowest-priority
	// fallback other than no-plan.
	Hint *ReceiverHint
	// ReceiverRequiresConstruction signals that the receiver type's zero
	// value is not meaningful — the struct holds unexported reference-typed
	// fields a constructor is expected to initialize. When set and no real
	// strategy (adapter, parameterless constructor, composite-literal-safe
	// shape, useful zero value, or operator hint) applies, PlanReceivers
	// returns UnsatisfiedRequirementKindRequiresConstruction instead of the
	// fallback zero-value plan, so the method is classified `unsupported`
	// rather than silently explored on a nil-state receiver (str-g7h7).
	ReceiverRequiresConstruction bool
	// MaxPlans caps the returned slice. Zero means DefaultMaxReceiverPlans.
	MaxPlans int
	// InterfaceImplsByParam maps constructor parameter names to discovered
	// interface implementation candidates. When a parameter has a usable
	// implementor constructor, it is satisfiable and does not consume a JSON
	// constructor input slot.
	InterfaceImplsByParam map[string][]protocol.InterfaceParamCandidate
}

// PlanReceivers returns a prioritised list of receiver plans for the method
// target t, capped at opts.MaxPlans (or DefaultMaxReceiverPlans). Strategy
// order is: adapter, same-package constructor, nearby-package constructor,
// composite literal, useful zero value, hint.
//
// When the target is not a method, PlanReceivers returns (nil, nil): free
// functions do not require a receiver plan.
//
// When no strategy applies, PlanReceivers returns (nil, u) where u describes
// the failure. Interface and generic-unconstrained receivers short-circuit
// to their matching UnsatisfiedRequirementKind without consulting strategies.
func PlanReceivers(t protocol.DiscoveredTarget, opts PlanOptions) ([]ReceiverPlan, *protocol.UnsatisfiedRequirement) {
	if t.Kind != protocol.TargetKindMethod {
		return nil, nil
	}
	if t.Receiver != nil && t.Receiver.IsInterface {
		return nil, &protocol.UnsatisfiedRequirement{
			Kind:     protocol.UnsatisfiedRequirementKindInterfaceReceiver,
			TargetID: t.ID,
			Detail:   fmt.Sprintf("receiver type %s is an interface", t.Receiver.TypeName),
		}
	}
	if t.HasTypeParams {
		if len(t.TypeParams) == 0 {
			return nil, &protocol.UnsatisfiedRequirement{
				Kind:     protocol.UnsatisfiedRequirementKindGenericUnconstrained,
				TargetID: t.ID,
				Detail:   "method has type parameters but no constraints were discovered",
			}
		}
		if _, unsat := PlanGenericTypeArgSets(t.ID, t.TypeParams); unsat != nil {
			return nil, unsat
		}
	}

	max := opts.MaxPlans
	if max <= 0 {
		max = DefaultMaxReceiverPlans
	}

	plans := make([]ReceiverPlan, 0, max)
	add := func(p ReceiverPlan) bool {
		if len(plans) >= max {
			return false
		}
		p.Priority = len(plans)
		plans = append(plans, p)
		return true
	}

	if opts.Adapter != nil {
		add(ReceiverPlan{
			Kind:         ReceiverPlanKindAdapter,
			ReceiverKind: opts.Adapter.ReceiverKind,
			Label:        defaultHintLabel(opts.Adapter, "adapter"),
		})
	}

	for _, c := range opts.SamePackageConstructors {
		if len(plans) >= max {
			break
		}
		// str-9b1q: allow parameterized constructors when all params are
		// primitive-satisfiable. The wrapper generates literal default values
		// for each constructor argument. Skip only when a param is not
		// satisfiable (opaque type, interface, etc.).
		if isParameterful(c) && !allParamsSatisfiable(c, opts.InterfaceImplsByParam) {
			continue
		}
		plan := ReceiverPlan{
			Kind:                             ReceiverPlanKindSamePackageConstructor,
			ReceiverKind:                     WrapperReceiverKindConstructorPrefix + c.FuncName,
			Label:                            labelForConstructor(c),
			ConstructorInterfaceImplsByParam: opts.InterfaceImplsByParam,
		}
		if isParameterful(c) {
			plan.ConstructorParams = c.Parameters
		}
		add(plan)
	}

	for _, c := range opts.NearbyPackageConstructors {
		if len(plans) >= max {
			break
		}
		if isParameterful(c) && !allParamsSatisfiable(c, opts.InterfaceImplsByParam) {
			continue
		}
		plan := ReceiverPlan{
			Kind:                             ReceiverPlanKindNearbyPackageConstructor,
			ReceiverKind:                     WrapperReceiverKindConstructorPrefix + c.FuncName,
			Label:                            labelForConstructor(c),
			ConstructorInterfaceImplsByParam: opts.InterfaceImplsByParam,
		}
		if isParameterful(c) {
			plan.ConstructorParams = c.Parameters
		}
		add(plan)
	}

	if opts.ReceiverIsCompositeLiteralSafe && t.Receiver != nil && !t.Receiver.IsPointer {
		add(ReceiverPlan{
			Kind:         ReceiverPlanKindCompositeLiteral,
			ReceiverKind: WrapperReceiverKindZeroValue,
			Label:        "composite_literal_" + toSnakeCase(t.Receiver.TypeName),
		})
	}

	if t.Receiver != nil {
		if _, ok := UsefulZeroValueTypes[t.Receiver.TypeName]; ok {
			add(ReceiverPlan{
				Kind:         ReceiverPlanKindUsefulZeroValue,
				ReceiverKind: WrapperReceiverKindZeroValue,
				Label:        "zero_value_" + toSnakeCase(t.Receiver.TypeName),
			})
		}
	}

	if opts.Hint != nil {
		add(ReceiverPlan{
			Kind:         ReceiverPlanKindHint,
			ReceiverKind: opts.Hint.ReceiverKind,
			Label:        defaultHintLabel(opts.Hint, "hint"),
		})
	}

	// When no real strategy fired and the caller has signalled that the
	// receiver type holds unexported reference-typed fields a constructor is
	// expected to initialize, refuse to emit the fallback zero-value plan.
	// Reporting zero-value-only nil-pointer panics for such methods would
	// not reflect call sites; the method is classified `unsupported` with
	// a specific reason instead (str-g7h7).
	if len(plans) == 0 && opts.ReceiverRequiresConstruction {
		return nil, &protocol.UnsatisfiedRequirement{
			Kind:     protocol.UnsatisfiedRequirementKindRequiresConstruction,
			TargetID: t.ID,
			Detail:   requiresConstructionDetail(t.Receiver),
		}
	}

	// Last-resort fallback: when no strategy fired, but the receiver type
	// is non-interface and non-generic-unconstrained, the wrapper's
	// `zero_value` switch case always compiles. Emit a fallback zero-value
	// plan so pointer receivers like `(*Config).Server` get an executable
	// plan instead of a `NoConstructor` unsatisfied requirement
	// (str-qo1.9).
	if len(plans) == 0 && fallbackZeroValueIsSafe(t) {
		add(ReceiverPlan{
			Kind:         ReceiverPlanKindFallbackZeroValue,
			ReceiverKind: WrapperReceiverKindZeroValue,
			Label:        "fallback_zero_value_" + toSnakeCase(t.Receiver.TypeName),
		})
	}

	if len(plans) == 0 {
		return nil, &protocol.UnsatisfiedRequirement{
			Kind:     protocol.UnsatisfiedRequirementKindNoConstructor,
			TargetID: t.ID,
			Detail:   receiverDetail(t.Receiver),
		}
	}
	return plans, nil
}

func requiresConstructionDetail(r *protocol.ReceiverShape) string {
	if r == nil || r.TypeName == "" {
		return "receiver requires constructor initialization (no parameterless constructor or hint available)"
	}
	return fmt.Sprintf("receiver type %s holds unexported reference-typed fields requiring constructor initialization; no parameterless constructor or hint available", r.TypeName)
}

// isParameterful reports whether c carries any function parameters. The
// wrapper template can only dispatch parameterless constructors (str-qo1.14),
// so the receiver planner mirrors that filter to keep its plan output and
// the wrapper switch in sync.
func isParameterful(c protocol.ConstructorCandidate) bool {
	return len(c.Parameters) > 0
}

// allParamsSatisfiable reports whether every parameter of c can be satisfied
// by the primitive family classifier or the runtime-value registry (str-9b1q).
// Used to permit parameterized constructors whose args are trivially
// generatable (string, int, float, bool, []byte, duration, uint).
func allParamsSatisfiable(c protocol.ConstructorCandidate, interfaceImplsByParam map[string][]protocol.InterfaceParamCandidate) bool {
	for _, p := range c.Parameters {
		if !isParamSatisfiable(p, interfaceImplsByParam) {
			return false
		}
	}
	return true
}

// fallbackZeroValueIsSafe reports whether a fallback zero-value receiver plan
// can compile and dispatch through the wrapper. The wrapper's `zero_value`
// case unconditionally writes `var _recv T` (or `&T{}` for pointer receivers)
// — both compile for any concrete struct. Interface receivers are guarded
// upstream and never reach this point. Generic targets without satisfiable
// type-args would already have short-circuited via PlanGenericTypeArgSets.
func fallbackZeroValueIsSafe(t protocol.DiscoveredTarget) bool {
	if t.Receiver == nil || t.Receiver.TypeName == "" {
		return false
	}
	if t.Receiver.IsInterface {
		return false
	}
	return true
}

func labelForConstructor(c protocol.ConstructorCandidate) string {
	name := c.FuncName
	switch name {
	case "New", "MustNew", "Default":
		name += c.TargetType
	}
	return toSnakeCase(name)
}

func defaultHintLabel(h *ReceiverHint, kind string) string {
	if h.Label != "" {
		return h.Label
	}
	token := strings.ReplaceAll(h.ReceiverKind, ":", "_")
	token = strings.ReplaceAll(token, ".", "_")
	if token == "" {
		return kind
	}
	return kind + "_" + toSnakeCase(token)
}

func receiverDetail(r *protocol.ReceiverShape) string {
	if r == nil {
		return "no constructor available"
	}
	return fmt.Sprintf("no constructor available for receiver %s", r.TypeName)
}

// toSnakeCase converts a CamelCase or mixed identifier to snake_case.
// Non-alphanumeric runes are normalised to underscores.
func toSnakeCase(s string) string {
	var b strings.Builder
	b.Grow(len(s) + 4)
	for i, r := range s {
		switch {
		case r >= 'A' && r <= 'Z':
			if i > 0 {
				prev := rune(s[i-1])
				if prev != '_' && !(prev >= 'A' && prev <= 'Z') {
					b.WriteByte('_')
				}
			}
			b.WriteRune(r + ('a' - 'A'))
		case (r >= 'a' && r <= 'z') || (r >= '0' && r <= '9'):
			b.WriteRune(r)
		default:
			b.WriteByte('_')
		}
	}
	out := b.String()
	for strings.Contains(out, "__") {
		out = strings.ReplaceAll(out, "__", "_")
	}
	return strings.Trim(out, "_")
}
