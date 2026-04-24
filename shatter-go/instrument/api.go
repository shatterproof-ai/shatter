package instrument

import "encoding/json"

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

// Invoke runs the prepared harness once, opening or respawning its subprocess
// as needed.
func (h *PreparedHarness) Invoke(inputs []json.RawMessage, capture bool) (*ExecuteResult, error) {
	return ExecuteWithPreparedHarness(h, inputs, nil, capture)
}
