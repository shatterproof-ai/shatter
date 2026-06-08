package testdata

// LocalControlPlane mirrors the Zolem-style struct that surfaced str-g7h7:
// its zero value holds nil maps and a nil channel, so the receiver-bearing
// methods panic with nil-pointer-deref unless callers route through
// NewLocalControlPlane (which is parameterful and therefore not auto-usable
// by the wrapper).
type LocalControlPlane struct {
	store     map[string]string
	counters  map[string]int
	listeners chan struct{}
}

// NewLocalControlPlane is parameterful, so the planner's parameterless-only
// gate (str-qo1.14) skips it. The receiver type still requires construction,
// which is what str-g7h7's classifier must detect from the field shapes.
func NewLocalControlPlane(seed map[string]string) *LocalControlPlane {
	return &LocalControlPlane{
		store:     seed,
		counters:  map[string]int{},
		listeners: make(chan struct{}, 1),
	}
}

// ListProfiles ranges the store map. On a zero-value receiver it returns nil
// because ranging over a nil map is safe, so a method-sensitive construction
// guard should allow this method.
func (l *LocalControlPlane) ListProfiles() []string {
	out := make([]string, 0, len(l.store))
	for k := range l.store {
		out = append(out, k)
	}
	return out
}

// BumpCounter writes to an uninitialized map on a zero-value receiver and must
// still be rejected by the construction guard.
func (l *LocalControlPlane) BumpCounter(name string) {
	l.counters[name]++
}

type entryDetails struct {
	id string
}

type OptionalEntry struct {
	details *entryDetails
	label   string
}

func (e OptionalEntry) Label() string {
	if e.details != nil {
		return e.details.id
	}
	return e.label
}

// PrimitiveOnly is the negative-control case: its zero value is well defined
// (numeric and string defaults), so str-g7h7 must NOT classify methods on
// PrimitiveOnly as requires_construction — they continue to receive the
// fallback zero-value plan.
type PrimitiveOnly struct {
	count int
	label string
}

// Describe reads only primitive fields and is safe to call on the zero value.
func (p PrimitiveOnly) Describe() string { return p.label }
