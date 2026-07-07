// Package runtimeval holds the registry of Go-source expressions the
// planner and the wrapper share to satisfy parameters that cannot be
// expressed as JSON literals (str-gxjs.1).
//
// The package is intentionally a leaf — it imports no other shatter-go
// packages — so both `shatter-go/planner` (which consumes the registry
// when emitting `Kind: runtime_value` ValuePlans) and `shatter-go/wrapper`
// (which bakes the expression into the generated wrapper at code-gen
// time, so the param-init site evaluates the expression instead of
// decoding from JSON inputs) can depend on it without forming a cycle.
//
// The protocol package's `SideEffectClass` is mirrored as a free-form
// string here to avoid pulling protocol into the leaf. Callers that
// need the strongly typed form convert at the boundary.
package runtimeval

import (
	"sort"
)

// wazeroCompiledModuleExpression builds a tiny module with one exported memory
// and one match function. It mirrors the scan-process ownership used for
// wazero.Runtime: Shatter currently has no runtime-value teardown hook, so the
// generated runtime and compiled module live for the wrapper process lifetime.
const wazeroCompiledModuleExpression = `func() wazero.CompiledModule {
	ctx := context.Background()
	rt := wazero.NewRuntime(ctx)
	compiled, err := rt.CompileModule(ctx, []byte{
		0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
		0x01, 0x07, 0x01, 0x60, 0x02, 0x7f, 0x7f, 0x01,
		0x7d, 0x03, 0x02, 0x01, 0x00, 0x05, 0x03, 0x01,
		0x00, 0x01, 0x07, 0x12, 0x02, 0x06, 0x6d, 0x65,
		0x6d, 0x6f, 0x72, 0x79, 0x02, 0x00, 0x05, 0x6d,
		0x61, 0x74, 0x63, 0x68, 0x00, 0x00, 0x0a, 0x09,
		0x01, 0x07, 0x00, 0x43, 0x00, 0x00, 0x80, 0x3f, 0x0b,
	})
	if err != nil {
		panic(err)
	}
	return compiled
}()`

// Candidate is a single registered expression for a parameter type that
// cannot be expressed as a JSON literal.
//
// Expression is the Go-source expression a wrapper pastes verbatim at
// the argument position (or that the planner returns as the Literal on
// a `Kind: runtime_value` ValuePlan). Imports is the unique, sorted set
// of package paths Expression references. SideEffectClass mirrors
// `protocol.SideEffectClass` as a string; empty entries default to the
// "pure" class at the planner boundary.
type Candidate struct {
	Expression      string
	TypeHint        string
	Imports         []string
	SideEffectClass string
}

// SymbolicCandidate registers a parameter type that is constructed at the
// wrapper's param-init site from a *symbolic* input slot rather than bound to
// a fixed runtime-value Expression. It is the type-keyed single source the
// analyzer, planner, and wrapper all consult so the symbolic-slot decision
// cannot drift between layers (str-ijtww). Drift is corrupting rather than
// merely wrong: a type added to only some layers shifts every subsequent
// param's input index.
//
// The canonical example is Go's `*http.Request` (str-e41w), whose body is
// driven symbolically so the solver can push request payloads past a handler's
// decode/validation guards. Registering the next framework type (e.g.
// `*gin.Context`, or a TS/Rust request type in those frontends) is a one-line
// addition here consumed consistently by all three layers.
//
// TypeHint is the Go-source type spelling (the registry key, e.g.
// "*http.Request"). Imports is the set of package paths Construction
// references. Construction lists the Go statements the wrapper emits to build
// the value: each entry is a fmt-style format string with two indexed verbs —
// %[1]s is the parameter variable name and %[2]s is the body-input variable
// holding the decoded symbolic string. The first statement must declare and
// assign the parameter variable; later statements (e.g. header stubs) may use
// only %[1]s.
type SymbolicCandidate struct {
	TypeHint     string
	Imports      []string
	Construction []string
}

// registry is the default set of Go parameter types the planner /
// wrapper can satisfy without user hints. Keyed by the Go-source
// spelling of the parameter type, including any leading `*` for
// pointer types.
//
// Earlier candidates are preferred; the order is stable to keep plan
// enumeration deterministic and to keep the wrapper picking the same
// expression across builds (str-gxjs.1).
var registry = map[string][]Candidate{
	"context.Context": {
		{
			Expression: "context.Background()",
			TypeHint:   "context.Context",
			Imports:    []string{"context"},
		},
	},
	"*bytes.Buffer": {
		{
			Expression: "&bytes.Buffer{}",
			TypeHint:   "*bytes.Buffer",
			Imports:    []string{"bytes"},
		},
	},
	"io.Reader": {
		{
			Expression: `strings.NewReader("")`,
			TypeHint:   "io.Reader",
			Imports:    []string{"strings"},
		},
	},
	"io.Writer": {
		{
			Expression: "&bytes.Buffer{}",
			TypeHint:   "io.Writer",
			Imports:    []string{"bytes"},
		},
		{
			Expression: "io.Discard",
			TypeHint:   "io.Writer",
			Imports:    []string{"io"},
		},
	},
	// str-gxjs: io.ReadCloser is the type Go's `http.Request.Body`
	// carries, so it appears in every handler-shaped scan. io.NopCloser
	// wraps an in-memory reader without touching OS file descriptors.
	"io.ReadCloser": {
		{
			Expression: `io.NopCloser(strings.NewReader(""))`,
			TypeHint:   "io.ReadCloser",
			Imports:    []string{"io", "strings"},
		},
	},
	"time.Time": {
		{
			Expression: "time.Time{}",
			TypeHint:   "time.Time",
			Imports:    []string{"time"},
		},
		{
			Expression: "time.Now()",
			TypeHint:   "time.Time",
			Imports:    []string{"time"},
		},
	},
	"http.Header": {
		{
			Expression: "http.Header{}",
			TypeHint:   "http.Header",
			Imports:    []string{"net/http"},
		},
	},
	// str-gxjs: http.ResponseWriter is synthesised via
	// httptest.NewRecorder, the canonical in-memory recorder used by
	// net/http tests. Safe — it does not bind a network socket.
	"http.ResponseWriter": {
		{
			Expression: "httptest.NewRecorder()",
			TypeHint:   "http.ResponseWriter",
			Imports:    []string{"net/http", "net/http/httptest"},
		},
	},
	"http.Handler": {
		{
			Expression: "http.NewServeMux()",
			TypeHint:   "http.Handler",
			Imports:    []string{"net/http"},
		},
	},
	// str-gxjs: *http.Request goes through httptest.NewRequest so the
	// Body / URL / method fields are populated by the stdlib's own
	// constructor rather than by composite-literal synthesis (which
	// can't fill unexported fields).
	"*http.Request": {
		{
			Expression: `httptest.NewRequest("GET", "/", bytes.NewReader(nil))`,
			TypeHint:   "*http.Request",
			Imports:    []string{"bytes", "net/http", "net/http/httptest"},
		},
	},
	// str-21k7: HTTP client/transport values use a generated in-memory
	// RoundTripper. It returns deterministic JSON responses and never delegates
	// to http.DefaultTransport, so target code can exercise HTTP success paths
	// without dialing the network.
	"*http.Client": {
		{
			Expression: "shatterHTTPClient()",
			TypeHint:   "*http.Client",
			Imports:    []string{"io", "net/http", "strings"},
		},
	},
	"http.RoundTripper": {
		{
			Expression: "shatterHTTPTransport()",
			TypeHint:   "http.RoundTripper",
			Imports:    []string{"io", "net/http", "strings"},
		},
	},
	"*template.Template": {
		{
			Expression: `template.Must(template.New("shatter").Parse("{}"))`,
			TypeHint:   "*template.Template",
			Imports:    []string{"text/template"},
		},
	},
	// str-ibba: wazero.Runtime is a live in-process WASM runtime. The default
	// constructor does not bind sockets or files; wrappers own it for the scan
	// process lifetime until runtime-value teardown support exists.
	"wazero.Runtime": {
		{
			Expression: `wazero.NewRuntime(context.Background())`,
			TypeHint:   "wazero.Runtime",
			Imports:    []string{"context", "github.com/tetratelabs/wazero"},
		},
	},
	// str-iek0: wazero.CompiledModule is backed by a live wazero runtime
	// compiled from a tiny deterministic WASM binary. The expression owns the
	// runtime for the wrapper process lifetime until teardown support exists.
	"wazero.CompiledModule": {
		{
			Expression: wazeroCompiledModuleExpression,
			TypeHint:   "wazero.CompiledModule",
			Imports:    []string{"context", "github.com/tetratelabs/wazero"},
		},
	},
}

// symbolicRegistry is the single source of the parameter types that are
// constructed from a symbolic input slot at the wrapper's param-init site,
// keyed by Go-source type spelling. All three layers (analyzer slot
// allocation, planner body-seed / hint handling, wrapper slot consumption)
// consult it so the decision stays single-sourced (str-ijtww).
//
// Adding the next framework type is a one-line registration here.
var symbolicRegistry = map[string]SymbolicCandidate{
	// str-e41w: a direct *http.Request param is built from a symbolic body
	// (a string input) via httptest.NewRequest rather than the fixed
	// empty-body runtime value. The method, path, and auth headers are fixed
	// so httptest.NewRequest cannot panic on an invalid verb and handlers do
	// not return before reading the body; the three common API auth
	// conventions (`x-api-key`, `Authorization: Bearer`, Google-style
	// `x-goog-api-key`) are stubbed so a presence-check on any passes. Only
	// the body is symbolic — that is what handler bodies read and branch on.
	"*http.Request": {
		TypeHint: "*http.Request",
		Imports:  []string{"net/http", "net/http/httptest", "strings"},
		Construction: []string{
			`var %[1]s *http.Request = httptest.NewRequest("POST", "/", strings.NewReader(%[2]s))`,
			`%[1]s.Header.Set("x-api-key", "shatter")`,
			`%[1]s.Header.Set("Authorization", "Bearer shatter")`,
			`%[1]s.Header.Set("x-goog-api-key", "shatter")`,
			`%[1]s.Header.Set("Content-Type", "application/json")`,
		},
	},
}

// LookupSymbolic returns the symbolic-construction candidate registered for
// the given Go-source type spelling and true, or a zero value and false when
// the type is not symbolic. The returned candidate is a fresh copy with
// imports normalised so callers may mutate it freely.
//
// The lookup is case-sensitive and matches the exact spelling.
func LookupSymbolic(typeName string) (SymbolicCandidate, bool) {
	entry, ok := symbolicRegistry[typeName]
	if !ok {
		return SymbolicCandidate{}, false
	}
	out := SymbolicCandidate{
		TypeHint:     entry.TypeHint,
		Imports:      sortedUniqueImports(entry.Imports),
		Construction: append([]string(nil), entry.Construction...),
	}
	return out, true
}

// IsSymbolic reports whether the given Go-source type spelling is constructed
// from a symbolic input slot (i.e. has a symbolicRegistry entry). Analyzer,
// planner, and wrapper all gate on this so the symbolic-slot decision is
// single-sourced.
func IsSymbolic(typeName string) bool {
	_, ok := symbolicRegistry[typeName]
	return ok
}

// SymbolicTypes returns the sorted list of type spellings registered as
// symbolic-construction candidates. Intended for diagnostics and tests.
func SymbolicTypes() []string {
	out := make([]string, 0, len(symbolicRegistry))
	for k := range symbolicRegistry {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

// Lookup returns the ordered runtime-value candidates registered for
// the given Go-source type spelling. The returned slice is a fresh
// copy with imports normalised so callers may mutate it freely.
//
// Unknown type spellings return nil. The lookup is case-sensitive and
// matches the exact spelling; it does not strip aliases or interface
// wrappers.
func Lookup(typeName string) []Candidate {
	entries, ok := registry[typeName]
	if !ok {
		return nil
	}
	out := make([]Candidate, len(entries))
	for i, e := range entries {
		out[i] = e
		out[i].Imports = sortedUniqueImports(e.Imports)
	}
	return out
}

// RegisteredTypes returns the sorted list of type spellings the
// registry currently recognises. Intended for diagnostics and tests.
func RegisteredTypes() []string {
	out := make([]string, 0, len(registry))
	for k := range registry {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

func sortedUniqueImports(paths []string) []string {
	if len(paths) == 0 {
		return nil
	}
	seen := make(map[string]struct{}, len(paths))
	for _, p := range paths {
		if p == "" {
			continue
		}
		seen[p] = struct{}{}
	}
	if len(seen) == 0 {
		return nil
	}
	out := make([]string, 0, len(seen))
	for p := range seen {
		out = append(out, p)
	}
	sort.Strings(out)
	return out
}
