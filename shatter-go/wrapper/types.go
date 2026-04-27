package wrapper

// TargetKind classifies a wrapper invocation target.
type TargetKind string

const (
	TargetKindFunction TargetKind = "function"
	TargetKindMethod   TargetKind = "method"
)

// ConstructorCandidate is the minimal constructor metadata needed by wrapper
// generation and build caching.
type ConstructorCandidate struct {
	FuncName   string
	TargetType string
}
