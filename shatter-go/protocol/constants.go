package protocol

// Error codes matching protocol/registry.yaml (12 codes).
const (
	ErrFileNotFound          = "file_not_found"
	ErrFunctionNotFound      = "function_not_found"
	ErrParseError            = "parse_error"
	ErrInstrumentationFailed = "instrumentation_failed"
	ErrExecutionTimeout      = "execution_timeout"
	ErrExecutionCrash        = "execution_crash"
	ErrVersionMismatch       = "version_mismatch"
	ErrInvalidRequest        = "invalid_request"
	ErrCompilationError      = "compilation_error"
	ErrInternalError         = "internal_error"
	ErrNotSupported          = "not_supported"
	// ErrPreflightFailed marks an environment preflight failure
	// (str-jeen.40). Wire-compatible; Go does not currently emit it
	// (see parity-matrix divergence
	// error-code-preflight-failed-typescript-only).
	ErrPreflightFailed = "preflight_failed"
)

// AllErrorCodes is provided by protocol_enums_gen.go (generated from
// protocol/registry.yaml). The hand-written ErrXxx constants above are
// reconciled against the generated list in generated_enums_test.go so a
// new error code cannot land in registry.yaml without a matching Err*
// constant (and vice versa).

// CommandCapabilities lists the standard protocol commands this frontend supports.
var CommandCapabilities = []string{"analyze", "execute", "instrument", "generate", "setup", "teardown", "prepare", "get_invocation_plan"}
