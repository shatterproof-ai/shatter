package instrument

import "testing"

// ── helpers ──────────────────────────────────────────────────────────────────

func constInt(n int64) *symExpr {
	return &symExpr{Kind: "const", Type: "int", Value: n}
}

func paramSym(name string) *symExpr {
	return &symExpr{Kind: "param", Name: name, Path: []string{}}
}

func gtZero(name string) *symExpr {
	return &symExpr{
		Kind:  "bin_op",
		Op:    "gt",
		Left:  paramSym(name),
		Right: constInt(0),
	}
}

// ── snapshot ─────────────────────────────────────────────────────────────────

func TestSnapshot_IsShallowCopy(t *testing.T) {
	orig := flowMap{"x": constInt(1), "y": constInt(2)}
	copy := snapshot(orig)

	// Mutating the copy must not affect the original.
	copy["z"] = constInt(3)
	if _, ok := orig["z"]; ok {
		t.Error("orig was mutated after copy was modified")
	}

	// Pointer values are shared (shallow).
	if orig["x"] != copy["x"] {
		t.Error("expected shared pointer for existing entry")
	}
}

func TestSnapshot_EmptyMap(t *testing.T) {
	orig := flowMap{}
	copy := snapshot(orig)
	if copy == nil {
		t.Error("snapshot of empty map returned nil")
	}
	if len(copy) != 0 {
		t.Errorf("len = %d, want 0", len(copy))
	}
}

// ── mergeFlowMaps — simple if/else divergence ─────────────────────────────────

func TestMergeFlowMaps_SimpleIfElseDivergence(t *testing.T) {
	// if x > 0 { label = 1 } else { label = -1 }
	cond := gtZero("x")
	baseMap := flowMap{"label": constInt(0)}
	thenMap := flowMap{"label": constInt(1)}
	elseMap := flowMap{"label": constInt(-1)}

	result := mergeFlowMaps(cond, thenMap, elseMap, baseMap)

	got, ok := result["label"]
	if !ok {
		t.Fatal("result missing key \"label\"")
	}
	if got.Kind != "ite" {
		t.Fatalf("kind = %q, want ite", got.Kind)
	}
	if got.Condition != cond {
		t.Errorf("condition pointer does not match cond")
	}
	wantThen := int64(1)
	if got.ThenExpr.Kind != "const" || got.ThenExpr.Value.(int64) != wantThen {
		t.Errorf("then_expr = %+v, want const int %d", got.ThenExpr, wantThen)
	}
	wantElse := int64(-1)
	if got.ElseExpr.Kind != "const" || got.ElseExpr.Value.(int64) != wantElse {
		t.Errorf("else_expr = %+v, want const int %d", got.ElseExpr, wantElse)
	}
}

// ── mergeFlowMaps — if-only (no else) ────────────────────────────────────────

func TestMergeFlowMaps_IfOnly_NoElse(t *testing.T) {
	// if x > 0 { label = 1 }
	// The caller should pass a copy of baseMap as elseMap when there is no
	// else clause. The pre-if value then becomes the else arm of the ite.
	cond := gtZero("x")
	preVal := constInt(0)
	baseMap := flowMap{"label": preVal}

	thenMap := flowMap{"label": constInt(1)}
	// No else clause: elseMap mirrors the base state for "label".
	// Passing elseMap without "label" (empty else) also works — the merge
	// function falls back to baseMap for the missing side.
	elseMap := flowMap{} // no modifications in the else branch

	result := mergeFlowMaps(cond, thenMap, elseMap, baseMap)

	got, ok := result["label"]
	if !ok {
		t.Fatal("result missing key \"label\"")
	}
	if got.Kind != "ite" {
		t.Fatalf("kind = %q, want ite", got.Kind)
	}
	if got.ThenExpr.Kind != "const" || got.ThenExpr.Value.(int64) != 1 {
		t.Errorf("then_expr = %+v, want const int 1", got.ThenExpr)
	}
	// else_expr falls back to the pre-if value (baseMap["label"]).
	if got.ElseExpr != preVal {
		t.Errorf("else_expr = %+v, want pre-if value %+v", got.ElseExpr, preVal)
	}
}

// ── mergeFlowMaps — nested if ─────────────────────────────────────────────────

func TestMergeFlowMaps_NestedIf(t *testing.T) {
	// if x > 0 {
	//     if y > 0 { z = 1 } else { z = 2 }
	// } else { z = 3 }
	condX := gtZero("x")
	condY := gtZero("y")
	baseMap := flowMap{"z": constInt(0)}

	// Inner merge (y > 0 branch inside then-branch of x > 0).
	innerBase := snapshot(baseMap)
	innerThen := flowMap{"z": constInt(1)}
	innerElse := flowMap{"z": constInt(2)}
	thenMap := mergeFlowMaps(condY, innerThen, innerElse, innerBase)

	// Outer merge.
	elseMap := flowMap{"z": constInt(3)}
	result := mergeFlowMaps(condX, thenMap, elseMap, baseMap)

	got, ok := result["z"]
	if !ok {
		t.Fatal("result missing key \"z\"")
	}
	if got.Kind != "ite" {
		t.Fatalf("outer kind = %q, want ite", got.Kind)
	}
	if got.Condition != condX {
		t.Errorf("outer condition does not match condX")
	}
	// then_expr must itself be an ite (inner merge result).
	if got.ThenExpr.Kind != "ite" {
		t.Errorf("then_expr.kind = %q, want ite (inner merge)", got.ThenExpr.Kind)
	}
	if got.ThenExpr.Condition != condY {
		t.Errorf("inner condition does not match condY")
	}
	// else_expr is the plain constant 3.
	if got.ElseExpr.Kind != "const" || got.ElseExpr.Value.(int64) != 3 {
		t.Errorf("else_expr = %+v, want const int 3", got.ElseExpr)
	}
}

// ── mergeFlowMaps — chained else-if ──────────────────────────────────────────

func TestMergeFlowMaps_ChainedElseIf(t *testing.T) {
	// if a > 0 { x = 1 } else if b > 0 { x = 2 } else { x = 3 }
	condA := gtZero("a")
	condB := gtZero("b")
	baseMap := flowMap{"x": paramSym("x")}

	// Inner else-if processed first.
	innerBase := snapshot(baseMap)
	innerThen := flowMap{"x": constInt(2)}
	innerElse := flowMap{"x": constInt(3)}
	elseMap := mergeFlowMaps(condB, innerThen, innerElse, innerBase)

	// Outer if.
	thenMap := flowMap{"x": constInt(1)}
	result := mergeFlowMaps(condA, thenMap, elseMap, baseMap)

	got, ok := result["x"]
	if !ok {
		t.Fatal("result missing key \"x\"")
	}
	if got.Kind != "ite" {
		t.Fatalf("outer kind = %q, want ite", got.Kind)
	}
	if got.Condition != condA {
		t.Errorf("outer condition does not match condA")
	}
	if got.ThenExpr.Kind != "const" || got.ThenExpr.Value.(int64) != 1 {
		t.Errorf("then_expr = %+v, want const int 1", got.ThenExpr)
	}
	// else_expr is itself an ite (b > 0 branch).
	if got.ElseExpr.Kind != "ite" {
		t.Errorf("else_expr.kind = %q, want ite (chained else-if)", got.ElseExpr.Kind)
	}
	if got.ElseExpr.Condition != condB {
		t.Errorf("else ite condition does not match condB")
	}
}

// ── mergeFlowMaps — no divergence (no ite emitted) ───────────────────────────

func TestMergeFlowMaps_NoDivergence_SamePointer(t *testing.T) {
	// Both branches carry the exact same *symExpr pointer.
	cond := gtZero("x")
	shared := constInt(42)
	baseMap := flowMap{"x": paramSym("x")}
	thenMap := flowMap{"result": shared}
	elseMap := flowMap{"result": shared}

	result := mergeFlowMaps(cond, thenMap, elseMap, baseMap)

	got, ok := result["result"]
	if !ok {
		t.Fatal("result missing key \"result\"")
	}
	if got.Kind == "ite" {
		t.Errorf("expected no ite when both branches share the same pointer")
	}
	if got != shared {
		t.Errorf("got %+v, want shared pointer %+v", got, shared)
	}
}

func TestMergeFlowMaps_NoDivergence_StructurallyEqual(t *testing.T) {
	// Both branches carry structurally identical but distinct *symExpr nodes.
	cond := gtZero("x")
	baseMap := flowMap{"x": paramSym("x")}
	thenMap := flowMap{"val": constInt(42)} // distinct allocation
	elseMap := flowMap{"val": constInt(42)} // same structure, different pointer

	result := mergeFlowMaps(cond, thenMap, elseMap, baseMap)

	got, ok := result["val"]
	if !ok {
		t.Fatal("result missing key \"val\"")
	}
	if got.Kind == "ite" {
		t.Errorf("expected no ite for structurally equal values")
	}
	if got.Kind != "const" {
		t.Errorf("kind = %q, want const", got.Kind)
	}
}

// ── mergeFlowMaps — unknown condition falls back to last-writer-wins ──────────

func TestMergeFlowMaps_UnknownCond_LastWriterWins(t *testing.T) {
	cond := &symExpr{Kind: "unknown"}
	baseMap := flowMap{"x": constInt(0)}
	thenMap := flowMap{"x": constInt(1), "a": constInt(10)}
	elseMap := flowMap{"x": constInt(2)}

	result := mergeFlowMaps(cond, thenMap, elseMap, baseMap)

	// "x": else-branch wins (2).
	if got := result["x"]; got == nil || got.Kind != "const" || got.Value.(int64) != 2 {
		t.Errorf("x = %+v, want const int 2 (else wins)", got)
	}
	// "a": only in then-branch; supplements because not in else.
	if got := result["a"]; got == nil || got.Kind != "const" || got.Value.(int64) != 10 {
		t.Errorf("a = %+v, want const int 10 (then supplements)", got)
	}
}

// ── mergeFlowMaps — nil condition treated same as unknown ────────────────────

func TestMergeFlowMaps_NilCond_LastWriterWins(t *testing.T) {
	baseMap := flowMap{"x": constInt(0)}
	thenMap := flowMap{"x": constInt(5)}
	elseMap := flowMap{"x": constInt(9)}

	result := mergeFlowMaps(nil, thenMap, elseMap, baseMap)

	if got := result["x"]; got == nil || got.Kind != "const" || got.Value.(int64) != 9 {
		t.Errorf("x = %+v, want const int 9 (else wins on nil cond)", got)
	}
}

// ── symExprsEqual ──────────────────────────────────────────────────────────────

func TestSymExprsEqual_SamePointer(t *testing.T) {
	a := constInt(1)
	if !symExprsEqual(a, a) {
		t.Error("symExprsEqual(a, a) = false, want true")
	}
}

func TestSymExprsEqual_Structurally(t *testing.T) {
	a := constInt(7)
	b := constInt(7)
	if a == b {
		t.Skip("pointers are unexpectedly equal (optimisation); test not meaningful")
	}
	if !symExprsEqual(a, b) {
		t.Error("symExprsEqual(a, b) = false for structurally identical nodes")
	}
}

func TestSymExprsEqual_Different(t *testing.T) {
	a := constInt(1)
	b := constInt(2)
	if symExprsEqual(a, b) {
		t.Error("symExprsEqual(a, b) = true for different values")
	}
}
