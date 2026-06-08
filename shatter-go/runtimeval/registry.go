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
