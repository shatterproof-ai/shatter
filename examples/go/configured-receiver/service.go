package configuredreceiver

type backend interface {
	Lookup(n int) string
}

type configuredBackend struct{}

func (configuredBackend) Lookup(n int) string {
	if n > 0 {
		return "configured-positive"
	}
	return "configured-nonpositive"
}

type constructorBackend struct{}

func (constructorBackend) Lookup(int) string {
	return "constructor"
}

type Service struct {
	backend backend
}

func NewService() *Service {
	return &Service{backend: constructorBackend{}}
}

func (s *Service) Classify(n int) string {
	if s == nil || s.backend == nil {
		return "missing-receiver"
	}
	label := s.backend.Lookup(n)
	if label == "configured-positive" {
		return "configured-positive"
	}
	if label == "configured-nonpositive" {
		return "configured-nonpositive"
	}
	return "wrong-receiver"
}
