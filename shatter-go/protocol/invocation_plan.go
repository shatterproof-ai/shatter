package protocol

import "encoding/json"

// ---------------------------------------------------------------------------
// Planner input types
// ---------------------------------------------------------------------------

// ValueRequirementKind specifies what kind of value is acceptable for a
// parameter during planner constraint solving.
type ValueRequirementKind string

const (
	// ValueRequirementKindAny accepts any value for the parameter.
	ValueRequirementKindAny ValueRequirementKind = "any"
	// ValueRequirementKindNonZero requires a non-zero value.
	ValueRequirementKindNonZero ValueRequirementKind = "non_zero"
	// ValueRequirementKindPositive requires a positive numeric value.
	ValueRequirementKindPositive ValueRequirementKind = "positive"
	// ValueRequirementKindSpecific requires the exact literal value in Literal.
	ValueRequirementKindSpecific ValueRequirementKind = "specific"
)

// ValueRequirement describes the constraint on one parameter for an
// invocation plan.
type ValueRequirement struct {
	// ParamIndex is the zero-based parameter index.
	ParamIndex int `json:"param_index"`
	// ParamName is the declared parameter name (may be empty for unnamed params).
	ParamName string `json:"param_name"`
	// TypeName is the Go type string for this parameter (e.g. "int", "*Counter").
	TypeName string `json:"type_name"`
	// Kind classifies the value constraint.
	Kind ValueRequirementKind `json:"kind"`
	// Literal is the required literal value when Kind is "specific".
	Literal json.RawMessage `json:"literal,omitempty"`
}

// RuntimeRequirementKind classifies runtime-setup requirements that must be
// satisfied before an invocation can proceed.
type RuntimeRequirementKind string

const (
	// RuntimeRequirementKindReceiverConstruction requires constructing a
	// method receiver before the invocation.
	RuntimeRequirementKindReceiverConstruction RuntimeRequirementKind = "receiver_construction"
	// RuntimeRequirementKindPackageInitialization requires package-level
	// initialization to have run.
	RuntimeRequirementKindPackageInitialization RuntimeRequirementKind = "package_initialization"
)

// RuntimeRequirement describes a runtime-setup precondition for an invocation.
type RuntimeRequirement struct {
	// Kind classifies the runtime requirement.
	Kind RuntimeRequirementKind `json:"kind"`
	// TypeName is the type involved in the requirement (e.g. a receiver type).
	TypeName string `json:"type_name,omitempty"`
	// Detail is a human-readable explanation of the requirement.
	Detail string `json:"detail,omitempty"`
}

// InvocationRequirement is the planner's input for one target. It captures
// what the planner must satisfy to produce an InvocationPlan.
type InvocationRequirement struct {
	// TargetID is the stable target identifier (e.g. "example.com/pkg:Add").
	TargetID string `json:"target_id"`
	// ValueRequirements lists the constraint for each parameter.
	ValueRequirements []ValueRequirement `json:"value_requirements"`
	// RuntimeRequirements lists any runtime-setup preconditions.
	RuntimeRequirements []RuntimeRequirement `json:"runtime_requirements,omitempty"`
}

// ---------------------------------------------------------------------------
// Planner output types
// ---------------------------------------------------------------------------

// ValuePlanKind describes how a value will be produced for an argument.
type ValuePlanKind string

const (
	// ValuePlanKindLiteral uses the exact literal in Literal.
	ValuePlanKindLiteral ValuePlanKind = "literal"
	// ValuePlanKindZero uses the zero value of the parameter type.
	ValuePlanKindZero ValuePlanKind = "zero"
	// ValuePlanKindRandom selects a random value from the type's value space.
	ValuePlanKindRandom ValuePlanKind = "random"
	// ValuePlanKindSymbolic marks the parameter as a symbolic variable tracked
	// by the concolic engine.
	ValuePlanKindSymbolic ValuePlanKind = "symbolic"
	// ValuePlanKindRuntimeValue sources the argument from the Go runtime-value
	// registry (e.g. context.Background() for context.Context). Literal is a
	// JSON-encoded string carrying the Go source expression; TypeHint names the
	// registered type so code generators can resolve package imports via
	// planner.LookupRuntimeValue.
	ValuePlanKindRuntimeValue ValuePlanKind = "runtime_value"
)

// ValuePlan describes how to produce a concrete value for one argument in an
// InvocationPlan.
type ValuePlan struct {
	// ParamIndex is the zero-based argument position.
	ParamIndex int `json:"param_index"`
	// ParamName is the declared parameter name (may be empty).
	ParamName string `json:"param_name"`
	// Kind specifies the value production strategy.
	Kind ValuePlanKind `json:"kind"`
	// Literal holds the concrete value when Kind is "literal".
	Literal json.RawMessage `json:"literal,omitempty"`
	// TypeHint carries the Go type string for code generation.
	TypeHint string `json:"type_hint,omitempty"`
}

// InvocationPlan is a complete, resolved plan for invoking a target once.
// It is the primary output of the planner for a satisfiable InvocationRequirement.
type InvocationPlan struct {
	// TargetID is the stable target identifier.
	TargetID string `json:"target_id"`
	// ReceiverKind selects how to construct the method receiver.
	// Use "zero_value" for zero-value construction, "initialized_maps" for a
	// receiver with wrapper-allocated map fields, "constructor:<FuncName>" for
	// a named constructor, or "" for free functions.
	ReceiverKind string `json:"receiver_kind"`
	// GenericTypeArgs is the ordered concrete type argument list for generic
	// targets, matching the target's TypeParams order.
	GenericTypeArgs []string `json:"generic_type_args,omitempty"`
	// ArgumentPlans is one ValuePlan per parameter, in declaration order.
	ArgumentPlans []ValuePlan `json:"argument_plans"`
	// ConstructorArgPlans holds ValuePlans for parameterized constructor
	// arguments (str-9b1q). When non-empty, the constructor named in
	// ReceiverKind takes these as positional arguments. The inputs array
	// sent to the wrapper prepends constructor arg values before method
	// param values, so the wrapper can split them by count.
	ConstructorArgPlans []ValuePlan `json:"constructor_arg_plans,omitempty"`
	// Priority is the relative ordering of this plan within a plan set.
	// Lower values are tried first.
	Priority int `json:"priority"`
	// Label is an optional human-readable name for this plan
	// (e.g. "zero_args", "constructor_new_counter").
	Label string `json:"label,omitempty"`
}

// ---------------------------------------------------------------------------
// Planning failure type
// ---------------------------------------------------------------------------

// UnsatisfiedRequirementKind classifies why a planner requirement could not
// be satisfied.
type UnsatisfiedRequirementKind string

const (
	// UnsatisfiedRequirementKindNoConstructor means no constructor is available
	// for the required receiver type.
	UnsatisfiedRequirementKindNoConstructor UnsatisfiedRequirementKind = "no_constructor"
	// UnsatisfiedRequirementKindInterfaceReceiver means the receiver type is an
	// interface and cannot be directly instantiated.
	UnsatisfiedRequirementKindInterfaceReceiver UnsatisfiedRequirementKind = "interface_receiver"
	// UnsatisfiedRequirementKindGenericUnconstrained means a generic type
	// parameter has no concrete instantiation available.
	UnsatisfiedRequirementKindGenericUnconstrained UnsatisfiedRequirementKind = "generic_unconstrained"
	// UnsatisfiedRequirementKindCGODependency means the package depends on cgo,
	// which blocks overlay-based compilation.
	UnsatisfiedRequirementKindCGODependency UnsatisfiedRequirementKind = "cgo_dependency"
	// UnsatisfiedRequirementKindComplexType means the parameter type is too
	// complex for the planner to synthesize a value for.
	UnsatisfiedRequirementKindComplexType UnsatisfiedRequirementKind = "complex_type"
	// UnsatisfiedRequirementKindRequiresConstruction means the receiver type's
	// zero value would not exercise meaningful behavior — the struct carries
	// unexported reference-typed fields (maps, channels, interfaces, function
	// values, or pointers) that a constructor is expected to initialize, and
	// no parameterless constructor or operator-supplied hint is available
	// (str-g7h7). Reporting zero-value-only nil-pointer panics for such
	// methods would not reflect real call sites; this kind signals that the
	// method should be classified `unsupported` until receiver setup is
	// configured.
	UnsatisfiedRequirementKindRequiresConstruction UnsatisfiedRequirementKind = "requires_construction"
)

// UnsatisfiedRequirement records a planning failure for one target, describing
// why the planner could not produce an InvocationPlan.
type UnsatisfiedRequirement struct {
	// Kind classifies the reason for the planning failure.
	Kind UnsatisfiedRequirementKind `json:"kind"`
	// TargetID is the target for which planning failed.
	TargetID string `json:"target_id"`
	// Detail is a human-readable explanation of the failure.
	Detail string `json:"detail,omitempty"`
}
