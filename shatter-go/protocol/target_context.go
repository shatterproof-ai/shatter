package protocol

// TargetContext bundles the per-target information the planner needs to plan
// for one InvocationRequirement. Building it requires more than the cached
// FunctionAnalysis: the receiver-aware planner pathway (str-hy9b.H5) reaches
// into the parsed package to recover Go-internal type information that is not
// shipped over the wire.
//
// TargetContext lives in the protocol package so that both protocol.PlannerFunc
// (in handler.go) and the planner package can reference it without creating an
// import cycle. The handler is the canonical producer; the planner is the sole
// consumer.
type TargetContext struct {
	// Analysis is the cached FunctionAnalysis for the target. Always populated
	// when the target_id resolved to a known analysis; nil when the target was
	// not previously analyzed.
	Analysis *FunctionAnalysis

	// Target is the Go-internal DiscoveredTarget reconstructed from the
	// parsed package. Populated for method targets so the planner can read
	// Receiver shape (TypeName, IsPointer, IsInterface) and HasTypeParams,
	// which are not carried on the wire FunctionAnalysis. Nil for free
	// functions and for callers that do not need receiver-aware planning.
	Target *DiscoveredTarget

	// Constructors lists same-package constructor candidates whose
	// TargetType matches Target.Receiver.TypeName. Populated alongside
	// Target by the handler's TargetContext builder; nil for free
	// functions and when no matching constructors are in scope.
	Constructors []ConstructorCandidate

	// ReceiverRequiresConstruction is set by the handler's TargetContext
	// builder when the method target's receiver type holds unexported
	// reference-typed fields (maps, channels, interfaces, function values,
	// pointers) a constructor is expected to initialize. The planner reads
	// this flag and refuses the fallback zero-value receiver plan when no
	// real strategy applies (str-g7h7).
	ReceiverRequiresConstruction bool

	// InterfaceImplsByParam maps parameter names to discovered interface
	// implementation candidates. Populated when a parameter is typed as an
	// imported interface whose defining package exports parameterless
	// constructors for implementing types (str-4v9h). The planner routes
	// these through PlanInterfaceImpls to produce runtime-value plans.
	InterfaceImplsByParam map[string][]InterfaceParamCandidate
}

// InterfaceParamCandidate names a concrete type that implements a parameter's
// interface type, together with its constructors. This is the protocol-level
// representation consumed by the planner's interface-impl planning path.
type InterfaceParamCandidate struct {
	// TypeName is the bare name of the concrete implementor (e.g. "FakerGenerator").
	TypeName string
	// SamePackage is true when the implementor is defined in the same
	// package as the target consuming the interface.
	SamePackage bool
	// Constructors lists the known constructor functions that produce
	// TypeName. For cross-package constructors, FuncName is package-
	// qualified (e.g. "response.NewFakerGenerator").
	Constructors []ConstructorCandidate
	// ImportPath is the package import path for cross-package constructors
	// (e.g. "internal/response"). Empty for same-package candidates.
	ImportPath string
}
