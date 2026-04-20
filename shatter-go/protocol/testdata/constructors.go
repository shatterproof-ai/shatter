package testdata

// Deps holds external dependencies for Service. Used by NewService to
// demonstrate that constructor parameters are captured with correct shapes.
type Deps struct {
	Host string
	Port int
}

// Service is a component assembled by NewService.
type Service struct {
	deps Deps
}

// NewService constructs a Service from its dependencies.
// Acceptance criterion: yields a ConstructorCandidate with
// target_type=Service, parameters=[{Name: deps, Type: object}], and
// returns_error=false.
func NewService(deps Deps) *Service {
	return &Service{deps: deps}
}

// Client makes remote connections.
type Client struct {
	host string
}

// MustNewClient constructs a Client (panics on internal failure).
// Acceptance criterion: yields a ConstructorCandidate with
// target_type=Client, parameters=[], and returns_error=false.
func MustNewClient() *Client {
	return &Client{host: "localhost"}
}
