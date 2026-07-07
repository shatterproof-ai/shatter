package runtimeval

import (
	"fmt"
	"sort"
	"strings"
	"testing"
)

// TestLookupSymbolic_HTTPRequest pins the *http.Request symbolic candidate so
// the single-sourced construction (str-ijtww) cannot silently change shape.
func TestLookupSymbolic_HTTPRequest(t *testing.T) {
	cand, ok := LookupSymbolic("*http.Request")
	if !ok {
		t.Fatal("LookupSymbolic(*http.Request) not found; symbolic registry regressed")
	}
	if cand.TypeHint != "*http.Request" {
		t.Errorf("TypeHint = %q, want %q", cand.TypeHint, "*http.Request")
	}
	wantImports := []string{"net/http", "net/http/httptest", "strings"}
	if !equalStrings(cand.Imports, wantImports) {
		t.Errorf("Imports = %v, want %v (sorted, deduped)", cand.Imports, wantImports)
	}
	if len(cand.Construction) == 0 {
		t.Fatal("Construction is empty; symbolic param would produce no value")
	}
	// The first statement must declare and assign the param variable from the
	// symbolic body slot.
	first := fmt.Sprintf(cand.Construction[0], "r", "body")
	for _, want := range []string{
		"var r *http.Request",
		`httptest.NewRequest("POST", "/", strings.NewReader(body))`,
	} {
		if !strings.Contains(first, want) {
			t.Errorf("Construction[0] = %q, missing %q", first, want)
		}
	}
	// The auth/content headers a presence-check handler branches on must be
	// stubbed.
	joined := renderConstruction(cand, "r", "body")
	for _, want := range []string{
		`r.Header.Set("x-api-key", "shatter")`,
		`r.Header.Set("Authorization", "Bearer shatter")`,
		`r.Header.Set("x-goog-api-key", "shatter")`,
		`r.Header.Set("Content-Type", "application/json")`,
	} {
		if !strings.Contains(joined, want) {
			t.Errorf("rendered construction missing %q\ngot:\n%s", want, joined)
		}
	}
}

// TestIsSymbolic_OnlySymbolicTypes verifies the symbolic gate is exact: it
// recognizes registered symbolic types and rejects runtime-value / unknown
// spellings. All three layers (analyzer, planner, wrapper) key off this, so a
// false positive would allocate a phantom input slot and shift every
// subsequent param's index.
func TestIsSymbolic_OnlySymbolicTypes(t *testing.T) {
	if !IsSymbolic("*http.Request") {
		t.Error("IsSymbolic(*http.Request) = false, want true")
	}
	for _, notSymbolic := range []string{
		"*template.Template", // registered as a runtime value, not symbolic
		"context.Context",
		"http.ResponseWriter",
		"*http.Client",
		"string",
		"",
		"http.Request", // missing leading * — exact match only
	} {
		if IsSymbolic(notSymbolic) {
			t.Errorf("IsSymbolic(%q) = true, want false", notSymbolic)
		}
	}
}

// TestSymbolicTypes_SortedAndConsistent verifies SymbolicTypes enumerates
// exactly the keys LookupSymbolic/IsSymbolic recognize.
func TestSymbolicTypes_SortedAndConsistent(t *testing.T) {
	types := SymbolicTypes()
	if !sort.StringsAreSorted(types) {
		t.Errorf("SymbolicTypes not sorted: %v", types)
	}
	for _, tn := range types {
		if !IsSymbolic(tn) {
			t.Errorf("SymbolicTypes lists %q but IsSymbolic reports false", tn)
		}
		if _, ok := LookupSymbolic(tn); !ok {
			t.Errorf("SymbolicTypes lists %q but LookupSymbolic reports not found", tn)
		}
	}
}

// TestLookupSymbolic_ReturnsIndependentCopy ensures callers (analyzer, wrapper)
// cannot corrupt the shared registry by mutating a returned candidate.
func TestLookupSymbolic_ReturnsIndependentCopy(t *testing.T) {
	first, ok := LookupSymbolic("*http.Request")
	if !ok {
		t.Fatal("LookupSymbolic(*http.Request) not found")
	}
	if len(first.Imports) > 0 {
		first.Imports[0] = "corrupted"
	}
	if len(first.Construction) > 0 {
		first.Construction[0] = "corrupted"
	}
	second, _ := LookupSymbolic("*http.Request")
	if len(second.Imports) == 0 || second.Imports[0] == "corrupted" {
		t.Error("mutating returned Imports corrupted the shared registry")
	}
	if len(second.Construction) == 0 || second.Construction[0] == "corrupted" {
		t.Error("mutating returned Construction corrupted the shared registry")
	}
}

// TestSymbolicRegistry_ConstructionSlotWellFormed verifies every registered
// symbolic candidate declares its param variable in the first statement and
// consumes the body slot exactly once, so the wrapper's uniform slot
// scaffolding lines up with each candidate's construction.
func TestSymbolicRegistry_ConstructionSlotWellFormed(t *testing.T) {
	for _, tn := range SymbolicTypes() {
		cand, _ := LookupSymbolic(tn)
		if len(cand.Construction) == 0 {
			t.Errorf("%q: empty Construction", tn)
			continue
		}
		if len(cand.Imports) == 0 {
			t.Errorf("%q: no Imports declared for its construction", tn)
		}
		first := fmt.Sprintf(cand.Construction[0], "v", "b")
		if !strings.HasPrefix(first, "var v ") {
			t.Errorf("%q: Construction[0] must declare the param var, got %q", tn, first)
		}
		body := renderConstruction(cand, "v", "b")
		if strings.Count(body, " b)") == 0 && !strings.Contains(body, "(b)") && !strings.Contains(body, "(b,") {
			t.Errorf("%q: construction never references the body slot; symbolic input would be dropped\ngot:\n%s", tn, body)
		}
	}
}

func renderConstruction(cand SymbolicCandidate, name, body string) string {
	var b strings.Builder
	for _, stmt := range cand.Construction {
		fmt.Fprintf(&b, "%s\n", fmt.Sprintf(stmt, name, body))
	}
	return b.String()
}

func equalStrings(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}
