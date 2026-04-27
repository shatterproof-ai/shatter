package instrument

import "time"

// ExecTimeout returns the per-invocation execution timeout. Reads
// SHATTER_EXEC_TIMEOUT (seconds, float) with a 5-second default. See the
// Timeout Contract in shatter-go/CLAUDE.md.
func ExecTimeout() time.Duration {
	return execTimeout()
}

// HarnessRuntimeModuleName is the module path used by generated launcher and
// harness builds when importing the shared runtime support package.
const HarnessRuntimeModuleName = harnessRuntimeModuleName

// EnsureHarnessRuntimeDir materializes the shared harness runtime module and
// returns the directory path that should be used in a replace directive.
func EnsureHarnessRuntimeDir() (string, error) {
	return ensureHarnessRuntimeDir()
}

// GenerateRecorder returns the recorder support source for the supplied
// package name.
func GenerateRecorder(packageName string) string {
	return generateRecorder(packageName)
}

// GenerateLoopMockFile returns the generated mock support source used by the
// loop-mode execution path.
func GenerateLoopMockFile(mocks []MockConfig) string {
	return generateLoopMockFile(mocks)
}

// DiscoverDependencies reports imported modules that are not covered by the
// current mock configuration.
func DiscoverDependencies(sourcePath string, mocks []MockConfig) []DiscoveredDependency {
	return discoverDependencies(sourcePath, mocks)
}
