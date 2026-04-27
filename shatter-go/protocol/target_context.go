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
}
