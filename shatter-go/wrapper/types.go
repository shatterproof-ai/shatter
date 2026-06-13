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
	// Parameters holds the Go type names for each constructor argument
	// (str-9b1q). When non-empty, wrapper generation emits a call with
	// arguments deserialized from _shatterInputs (offset before method
	// params). When empty (parameterless), the wrapper emits a plain
	// `NewFoo()` call.
	Parameters []ConstructorParam
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
	// ReturnsError is true when the constructor's signature includes a
	// trailing error return: (T, error) or (*T, error). Wrapper generation
	// must use a two-assignment form (_recv, _ := ctor()) instead of the
	// single-assignment form (_recv := ctor()) to avoid:
	//   assignment mismatch: 1 variable but ctor returns 2 values
	// See str-jeen.78.
	ReturnsError bool
	// ReturnsInterface is true when the constructor returns an interface
	// value backed by TargetType. Wrapper generation type-asserts the
	// interface result to the concrete receiver before invoking the method.
	ReturnsInterface bool
}

// ConstructorParam describes a single constructor argument for wrapper
// code generation (str-9b1q).
type ConstructorParam struct {
	// Name is the declared parameter name.
	Name string
	// GoType is the Go source type spelling (e.g. "string", "int").
	GoType string
	// RuntimeValueExpr, when non-empty, carries a Go-source expression
	// assignable to GoType. Constructor params with this field set are
	// initialized directly and do not consume a JSON input slot.
	RuntimeValueExpr string
	// Imports lists package paths required by RuntimeValueExpr.
	Imports []string
}
