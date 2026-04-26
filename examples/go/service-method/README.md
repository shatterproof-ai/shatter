# service-method (str-hy9b.H5)

E2E fixture for the receiver-aware planner pathway: `*Service.Compute` with a
same-package `New()` constructor, used by `shatter-core/tests/e2e_concolic.rs`
to drive analyze → plan → execute against the Go frontend.

Lives at top-level (`example.com/service-method`) rather than under
`examples/go/internal-method/internal/svc/` because the launcher synthesizes
its harness module outside the target's tree and Go's `internal/` rule would
block the import. The internal-method fixture covers C2/D7 separately.
