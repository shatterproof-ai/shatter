package instrument

import "testing"

// str-djcv2 review: parseMockSymbol folds the wire colon form, so a
// single-segment module ("module:Export") now registers as a bare QUALIFIER
// rather than an import path. For a real single-segment import path the local
// name equals the path, so suppression still fires for the intended import —
// but it also suppresses any unrelated import whose local name happens to
// match, the documented over-suppression limit of bare spellings. Pinned here
// so a future change to the matcher classes is a deliberate one.
func TestParseMockSymbol_WireFormSingleSegmentModule(t *testing.T) {
	p, ok := parseMockSymbol("module:Export")
	if !ok {
		t.Fatalf("wire form should parse")
	}
	if p.ImportPath != "" {
		t.Errorf("single-segment wire form should not be path-qualified, got ImportPath=%q", p.ImportPath)
	}
	if p.Base != "module" || p.Func != "Export" {
		t.Errorf("got Base=%q Func=%q, want module/Export", p.Base, p.Func)
	}

	// The multi-segment wire form stays path-qualified (exact identity).
	q, ok := parseMockSymbol("example.com/module:Export")
	if !ok {
		t.Fatalf("path-qualified wire form should parse")
	}
	if q.ImportPath != "example.com/module" || q.Base != "module" || q.Func != "Export" {
		t.Errorf("got %+v, want ImportPath=example.com/module Base=module Func=Export", q)
	}
}
