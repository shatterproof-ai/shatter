package protocol

// Error codes matching protocol/registry.yaml (11 codes).
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
)

// AllErrorCodes lists all valid error codes for parity testing.
var AllErrorCodes = []string{
	ErrFileNotFound, ErrFunctionNotFound, ErrParseError,
	ErrInstrumentationFailed, ErrExecutionTimeout, ErrExecutionCrash,
	ErrVersionMismatch, ErrInvalidRequest, ErrCompilationError,
	ErrInternalError, ErrNotSupported,
}

// CommandCapabilities lists the standard protocol commands this frontend supports.
var CommandCapabilities = []string{"analyze", "execute", "instrument", "generate", "setup", "teardown", "prepare", "get_invocation_plan"}
