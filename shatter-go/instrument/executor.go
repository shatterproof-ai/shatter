package instrument

import (
	"encoding/json"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"math"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"time"
)

const defaultExecTimeout = 5 * time.Second

// maxTimeoutSecs guards against overflow when converting float64 seconds to
// time.Duration. time.Duration is int64 nanoseconds (max ≈292 years); we cap
// at 24 hours which is well beyond any realistic execution timeout.
const maxTimeoutSecs = 86400

// Mock behavior constants matching the protocol's DefaultBehavior field.
const (
	BehaviorRepeatLast  = "repeat_last"
	BehaviorCycle       = "cycle"
	BehaviorThrowError  = "throw_error"
	BehaviorPassthrough = "passthrough"
	MockErrorPrefix     = "Mock error: "
)

// harnessRuntimeModuleName is the Go module path used by the shared harness
// runtime package. Generated harness binaries import this module and resolve it
// to the checked-in shatter-go/harness module via a replace directive.
const harnessRuntimeModuleName = "shatter-harness"

// workspaceGoEnvProvider, when non-nil, returns the environment slice to use
// for every `go build` invoked from shatter-go. The protocol handler installs
// it so GOCACHE is pinned to the workspace-backed build cache. When nil
// (e.g., in unit tests that don't wire a workspace), callers fall back to
// the host environment.
var workspaceGoEnvProvider func() []string

// harnessRuntimeOnce caches the resolved path to the checked-in harness module
// so repeated builds do not need to rediscover it.
var (
	harnessRuntimeOnce sync.Once
	harnessRuntimeDir  string
	harnessRuntimeErr  error
)

// subprocessPackages lists Go packages that spawn external processes.
var subprocessPackages = map[string]bool{
	"os/exec": true,
}

// execTimeout returns the execution timeout, reading from SHATTER_EXEC_TIMEOUT
// env var (in seconds) with a fallback to defaultExecTimeout.
func execTimeout() time.Duration {
	if d, ok := parseTimeoutEnv("SHATTER_EXEC_TIMEOUT"); ok {
		return d
	}
	return defaultExecTimeout
}

// isMcdcEnabled returns true when MC/DC mode is enabled (SHATTER_MCDC=1).
// Follows the same pattern as execTimeout() for SHATTER_EXEC_TIMEOUT.
func isMcdcEnabled() bool {
	return os.Getenv("SHATTER_MCDC") == "1"
}

// parseTimeoutEnv reads an env var as seconds and returns a valid duration.
// Returns false for missing, non-numeric, non-positive, overflow, or sub-
// nanosecond values — callers fall back to their default.
func parseTimeoutEnv(key string) (time.Duration, bool) {
	s := os.Getenv(key)
	if s == "" {
		return 0, false
	}
	secs, err := strconv.ParseFloat(s, 64)
	if err != nil || secs <= 0 || math.IsInf(secs, 0) || math.IsNaN(secs) || secs >= maxTimeoutSecs {
		return 0, false
	}
	d := time.Duration(secs * float64(time.Second))
	if d <= 0 {
		return 0, false
	}
	return d, true
}

// SetWorkspaceGoEnvProvider installs the environment provider used for `go
// build` invocations. Passing nil disables workspace-backed GOCACHE pinning.
func SetWorkspaceGoEnvProvider(fn func() []string) {
	workspaceGoEnvProvider = fn
}

// WorkspaceGoEnv returns the workspace-backed environment slice when a
// provider has been installed, or nil otherwise. Callers outside this package
// (notably setup.Loader) use the nil signal to decide whether to pin GOCACHE.
func WorkspaceGoEnv() []string {
	if workspaceGoEnvProvider == nil {
		return nil
	}
	return workspaceGoEnvProvider()
}

// ensureHarnessRuntimeDir returns the absolute path to the checked-in
// shatter-go/harness module so generated launcher builds can import
// shatter-harness through a stable local replace target.
func ensureHarnessRuntimeDir() (string, error) {
	harnessRuntimeOnce.Do(func() {
		_, currentFile, _, ok := runtime.Caller(0)
		if !ok {
			harnessRuntimeErr = fmt.Errorf("locating instrument package source")
			return
		}

		moduleDir := filepath.Clean(filepath.Join(filepath.Dir(currentFile), "..", "harness"))
		absModuleDir, err := filepath.Abs(moduleDir)
		if err != nil {
			harnessRuntimeErr = fmt.Errorf("resolving harness runtime dir: %w", err)
			return
		}
		if _, err := os.Stat(filepath.Join(absModuleDir, "go.mod")); err != nil {
			harnessRuntimeErr = fmt.Errorf("stat harness runtime go.mod: %w", err)
			return
		}
		harnessRuntimeDir = absModuleDir
	})
	return harnessRuntimeDir, harnessRuntimeErr
}

// generateLoopMockFile generates the mock support source consumed by the
// loop-mode execution path. It emits per-mock function variables, per-loop
// counter resets, and a call recorder that flushes recorded mock calls into
// the response.
func generateLoopMockFile(mocks []MockConfig) string {
	hasThrowError := false
	for _, m := range mocks {
		if m.DefaultBehavior == BehaviorThrowError {
			hasThrowError = true
			break
		}
	}

	var b strings.Builder

	b.WriteString("package main\n\n")
	b.WriteString("import (\n")
	b.WriteString("\t\"encoding/json\"\n")
	if hasThrowError {
		b.WriteString("\t\"fmt\"\n")
	}
	b.WriteString("\t\"sync\"\n")
	b.WriteString("\t\"sync/atomic\"\n")
	b.WriteString(")\n\n")

	b.WriteString("type shatterMockCall struct {\n")
	b.WriteString("\tSymbol      string          `json:\"symbol\"`\n")
	b.WriteString("\tArgs        json.RawMessage `json:\"args\"`\n")
	b.WriteString("\tReturnValue json.RawMessage `json:\"return_value\"`\n")
	b.WriteString("}\n\n")

	b.WriteString("var (\n")
	b.WriteString("\tshatterMockCalls   []shatterMockCall\n")
	b.WriteString("\tshatterMockCallsMu sync.Mutex\n")
	b.WriteString(")\n\n")

	b.WriteString("func shatterRecordMockCall(symbol string, args any, retVal any) {\n")
	b.WriteString("\targsJSON, _ := json.Marshal(args)\n")
	b.WriteString("\tretJSON, _ := json.Marshal(retVal)\n")
	b.WriteString("\tshatterMockCallsMu.Lock()\n")
	b.WriteString("\tshatterMockCalls = append(shatterMockCalls, shatterMockCall{\n")
	b.WriteString("\t\tSymbol:      symbol,\n")
	b.WriteString("\t\tArgs:        argsJSON,\n")
	b.WriteString("\t\tReturnValue: retJSON,\n")
	b.WriteString("\t})\n")
	b.WriteString("\tshatterMockCallsMu.Unlock()\n")
	b.WriteString("}\n\n")

	// shatterResetMockCounters resets all per-mock call indices and the accumulated
	// call list so each loop iteration starts from a clean state.
	b.WriteString("func shatterResetMockCounters() {\n")
	for i := range mocks {
		fmt.Fprintf(&b, "\tatomic.StoreInt64(&shatterMock%d_callIdx, 0)\n", i)
	}
	b.WriteString("\tshatterMockCallsMu.Lock()\n")
	b.WriteString("\tshatterMockCalls = shatterMockCalls[:0]\n")
	b.WriteString("\tshatterMockCallsMu.Unlock()\n")
	b.WriteString("}\n\n")

	// shatterGetAndResetMockCalls returns the recorded calls as raw JSON and clears
	// the list so the next iteration starts fresh.
	b.WriteString("func shatterGetAndResetMockCalls() []json.RawMessage {\n")
	b.WriteString("\tshatterMockCallsMu.Lock()\n")
	b.WriteString("\tdefer shatterMockCallsMu.Unlock()\n")
	b.WriteString("\tif len(shatterMockCalls) == 0 {\n")
	b.WriteString("\t\treturn nil\n")
	b.WriteString("\t}\n")
	b.WriteString("\tout := make([]json.RawMessage, len(shatterMockCalls))\n")
	b.WriteString("\tfor i, c := range shatterMockCalls {\n")
	b.WriteString("\t\tout[i], _ = json.Marshal(c)\n")
	b.WriteString("\t}\n")
	b.WriteString("\tshatterMockCalls = shatterMockCalls[:0]\n")
	b.WriteString("\treturn out\n")
	b.WriteString("}\n\n")

	for i, mock := range mocks {
		if mock.DefaultBehavior == BehaviorPassthrough {
			continue
		}

		safeName := sanitizeMockName(mock.Symbol)
		retValsJSON, _ := json.Marshal(mock.ReturnValues)

		fmt.Fprintf(&b, "// Mock for %s\n", mock.Symbol)
		fmt.Fprintf(&b, "var shatterMock%d_retvals = func() []json.RawMessage {\n", i)
		b.WriteString("\tvar vals []any\n")
		fmt.Fprintf(&b, "\tjson.Unmarshal([]byte(`%s`), &vals)\n", string(retValsJSON))
		b.WriteString("\tresult := make([]json.RawMessage, len(vals))\n")
		b.WriteString("\tfor i, v := range vals {\n")
		b.WriteString("\t\tresult[i], _ = json.Marshal(v)\n")
		b.WriteString("\t}\n")
		b.WriteString("\treturn result\n")
		b.WriteString("}()\n")
		fmt.Fprintf(&b, "var shatterMock%d_callIdx int64\n\n", i)

		if mock.DefaultBehavior == BehaviorThrowError {
			fmt.Fprintf(&b, "func ShatterMock_%s(args ...any) any {\n", safeName)
			fmt.Fprintf(&b, "\tretvals := shatterMock%d_retvals\n", i)
			fmt.Fprintf(&b, "\tidx := int(atomic.AddInt64(&shatterMock%d_callIdx, 1)) - 1\n", i)
			b.WriteString("\tif idx >= len(retvals) && len(retvals) > 0 {\n")
			b.WriteString("\t\tidx = len(retvals) - 1\n")
			b.WriteString("\t}\n")
			fmt.Fprintf(&b, "\tmsg := %q\n", MockErrorPrefix+mock.Symbol)
			b.WriteString("\tif idx < len(retvals) {\n")
			b.WriteString("\t\tvar obj map[string]any\n")
			b.WriteString("\t\tif json.Unmarshal(retvals[idx], &obj) == nil {\n")
			b.WriteString("\t\t\tif s, ok := obj[\"message\"].(string); ok && s != \"\" {\n")
			b.WriteString("\t\t\t\tmsg = s\n")
			b.WriteString("\t\t\t}\n")
			b.WriteString("\t\t}\n")
			b.WriteString("\t}\n")
			if mock.ShouldTrackCalls {
				fmt.Fprintf(&b, "\tshatterRecordMockCall(%q, args, msg)\n", mock.Symbol)
			}
			b.WriteString("\tpanic(msg)\n")
			b.WriteString("}\n\n")

			fmt.Fprintf(&b, "func ShatterMockErr_%s(args ...any) (any, error) {\n", safeName)
			fmt.Fprintf(&b, "\tretvals := shatterMock%d_retvals\n", i)
			fmt.Fprintf(&b, "\tidx := int(atomic.AddInt64(&shatterMock%d_callIdx, 1)) - 1\n", i)
			b.WriteString("\tif idx >= len(retvals) && len(retvals) > 0 {\n")
			b.WriteString("\t\tidx = len(retvals) - 1\n")
			b.WriteString("\t}\n")
			fmt.Fprintf(&b, "\tmsg := %q\n", MockErrorPrefix+mock.Symbol)
			b.WriteString("\tif idx < len(retvals) {\n")
			b.WriteString("\t\tvar obj map[string]any\n")
			b.WriteString("\t\tif json.Unmarshal(retvals[idx], &obj) == nil {\n")
			b.WriteString("\t\t\tif s, ok := obj[\"message\"].(string); ok && s != \"\" {\n")
			b.WriteString("\t\t\t\tmsg = s\n")
			b.WriteString("\t\t\t}\n")
			b.WriteString("\t\t}\n")
			b.WriteString("\t}\n")
			if mock.ShouldTrackCalls {
				fmt.Fprintf(&b, "\tshatterRecordMockCall(%q, args, msg)\n", mock.Symbol)
			}
			b.WriteString("\treturn nil, fmt.Errorf(\"%s\", msg)\n")
			b.WriteString("}\n\n")
			continue
		}

		fmt.Fprintf(&b, "func ShatterMock_%s(args ...any) any {\n", safeName)
		fmt.Fprintf(&b, "\tretvals := shatterMock%d_retvals\n", i)
		fmt.Fprintf(&b, "\tidx := int(atomic.AddInt64(&shatterMock%d_callIdx, 1)) - 1\n", i)
		if mock.DefaultBehavior == BehaviorRepeatLast || mock.DefaultBehavior == "" {
			b.WriteString("\tif idx >= len(retvals) && len(retvals) > 0 {\n")
			b.WriteString("\t\tidx = len(retvals) - 1\n")
			b.WriteString("\t}\n")
		} else {
			b.WriteString("\tif len(retvals) > 0 {\n")
			b.WriteString("\t\tidx = idx % len(retvals)\n")
			b.WriteString("\t}\n")
		}
		b.WriteString("\tvar retVal any\n")
		b.WriteString("\tif idx < len(retvals) {\n")
		b.WriteString("\t\tjson.Unmarshal(retvals[idx], &retVal)\n")
		b.WriteString("\t}\n")
		if mock.ShouldTrackCalls {
			fmt.Fprintf(&b, "\tshatterRecordMockCall(%q, args, retVal)\n", mock.Symbol)
		}
		b.WriteString("\treturn retVal\n")
		b.WriteString("}\n\n")
	}

	return b.String()
}

// sanitizeMockName converts a symbol name (e.g. "fs.readFile") to a valid Go identifier.
func sanitizeMockName(symbol string) string {
	result := make([]byte, 0, len(symbol))
	for _, c := range symbol {
		if (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_' {
			result = append(result, byte(c))
		} else {
			result = append(result, '_')
		}
	}
	return string(result)
}

// discoverDependencies inspects the source file's imports and reports any that
// are not covered by the provided mocks. Third-party packages (containing a dot
// in the import path) and known subprocess-spawning packages are reported.
func discoverDependencies(sourcePath string, mocks []MockConfig) []DiscoveredDependency {
	fset := token.NewFileSet()
	f, err := parser.ParseFile(fset, sourcePath, nil, parser.ImportsOnly)
	if err != nil {
		return nil
	}

	// Build the mocked-import matchers from mock symbols. Two symbol shapes:
	//   "module:export" / "module/path" — suppress by exact import path;
	//   "pkg.Func" (config-mock source qualifier, str-c8djq) — suppress any
	//   import whose local name (alias, or path base by Go convention) is the
	//   qualifier.
	mockedModules := make(map[string]bool)
	mockedQualifiers := make(map[string]bool)
	for _, m := range mocks {
		switch {
		case strings.Contains(m.Symbol, ":"):
			mockedModules[m.Symbol[:strings.Index(m.Symbol, ":")]] = true
		case !strings.Contains(m.Symbol, "/") && strings.Contains(m.Symbol, "."):
			mockedQualifiers[m.Symbol[:strings.Index(m.Symbol, ".")]] = true
		default:
			mockedModules[m.Symbol] = true
		}
	}
	importLocalName := func(imp *ast.ImportSpec, importPath string) string {
		if imp.Name != nil {
			return imp.Name.Name
		}
		if idx := strings.LastIndex(importPath, "/"); idx >= 0 {
			return importPath[idx+1:]
		}
		return importPath
	}

	var deps []DiscoveredDependency
	for _, imp := range f.Imports {
		importPath := strings.Trim(imp.Path.Value, `"`)

		if mockedModules[importPath] || mockedQualifiers[importLocalName(imp, importPath)] {
			continue
		}

		if subprocessPackages[importPath] {
			deps = append(deps, DiscoveredDependency{
				Symbol:            importPath,
				SourceModule:      importPath,
				Kind:              "subprocess_spawn",
				IsSubprocessSpawn: true,
			})
			continue
		}

		// Report third-party packages (import paths containing a dot indicate
		// a domain-based module path, e.g. "github.com/...").
		if strings.Contains(importPath, ".") {
			deps = append(deps, DiscoveredDependency{
				Symbol:            importPath,
				SourceModule:      importPath,
				Kind:              "unmocked_import",
				IsSubprocessSpawn: false,
			})
		}
	}
	return deps
}
