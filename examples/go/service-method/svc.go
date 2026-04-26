// Package svc is the H5 (str-hy9b.H5) end-to-end fixture: a method-with-
// receiver target whose receiver has a same-package constructor and whose
// method body has a branch on the parameter. The package lives at
// `example.com/service-method` (NOT under `internal/...`) so the launcher
// module synthesized by shatter-go/launcher can import it without hitting
// Go's internal-visibility rule.
//
// The internal-method fixture in examples/go/internal-method exists for the
// orthogonal C2/D7 internal-package coverage; receiver-aware planner-driven
// E2E lives here because internal-package visibility is not part of the H5
// AC and the launcher's synthetic module cannot satisfy it.
package svc

// Service is a minimal stateful service type with a same-package
// constructor `New` so the H5 receiver planner can compose a
// `constructor:New` plan against it.
type Service struct{}

// New returns a new Service instance. Same-package constructor
// candidates whose TargetType matches the receiver type drive the
// receiver-planner's `constructor:<FuncName>` strategy.
func New() *Service {
	return &Service{}
}

// Compute classifies the input x and returns a result code. The branch
// on x provides a concrete exploration target for the concolic engine
// and is the same shape the str-hy9b.C4 method-classification gate test
// uses (`*Service.Compute`).
func (s *Service) Compute(x int) int {
	if x > 0 {
		return 1
	}
	return -1
}
