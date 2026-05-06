package wrapper

// TargetKind classifies a wrapper invocation target.
type TargetKind string

const (
	TargetKindFunction TargetKind = "function"
	TargetKindMethod   TargetKind = "method"
)

// ConstructorCandidate is the minimal constructor metadata needed by wrapper
// generation and build caching.
//
// HasParams is true when the underlying constructor function takes one or
// more parameters. Wrapper generation cannot synthesise constructor
// arguments and therefore must skip the receiver-kind case for any
// constructor with HasParams set; emitting `_recv := NewFoo()` for a
// constructor whose real signature is `NewFoo(http.ResponseWriter) *Foo`
// produces a package-wide build error that poisons every other target in
// the same wrapper. See str-qo1.14.
type ConstructorCandidate struct {
	FuncName   string
	TargetType string
	HasParams  bool
	// ReturnsPointer reports whether the constructor's return type is the
	// pointer form (`*T`) or the value form (`T`). Wrapper generation
	// branches on the combination of receiver kind and this flag:
	//   ptr-recv + ptr-ret  →  _recv := NewT()
	//   ptr-recv + val-ret  →  _v := DefaultT(); _recv := &_v
	//   val-recv + ptr-ret  →  _recv := *NewT()
	//   val-recv + val-ret  →  _recv := DefaultT()
	// Pre-fix every val-recv case emitted `*ctor()`, which fails to
	// compile when the constructor returns a value (`cannot indirect`).
	// See str-jeen.49.
	ReturnsPointer bool
}
