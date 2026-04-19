// Package svc is an internal service package used to validate that the Shatter
// Go frontend can analyze and execute methods on types declared in internal/...
// packages without hitting Go's internal visibility restrictions.
package svc

// Service is a minimal stateful service type.
type Service struct{}

// New returns a new Service instance.
func New() *Service {
	return &Service{}
}

// DoIt classifies the input x and returns a result code.
// The branch on x provides a concrete exploration target for the concolic engine.
func (s *Service) DoIt(x int) int {
	if x > 0 {
		return 1
	}
	return -1
}
