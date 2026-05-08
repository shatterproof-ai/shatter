package instrument

import "encoding/json"

// flowMap tracks the symbolic value of each variable name in scope.
// It is used to propagate data-flow information through statement sequences
// and produce ite SymExprs at if/else merge points.
type flowMap map[string]*symExpr

// snapshot returns a shallow copy of m.  Because *symExpr nodes are
// immutable once built, a pointer-level copy is sufficient for tracking
// divergence across branches.
func snapshot(m flowMap) flowMap {
	out := make(flowMap, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}

// mergeFlowMaps applies SSA phi-node semantics to produce the merged state
// after an if/else statement.
//
//   - cond    – the symbolic condition of the if
//   - thenMap – flow map at the end of the then-branch
//   - elseMap – flow map at the end of the else-branch (pass a copy of
//     baseMap when no else clause is present)
//   - baseMap – snapshot taken immediately before the if statement
//
// For each variable name present in either branch:
//   - Identical values on both sides → keep as-is (no ite emitted).
//   - Value present on only one side and absent from baseMap → variable was
//     introduced inside that branch; keep the defining branch's value.
//   - Divergent values → emit ite{condition: cond, then_expr, else_expr}
//     where a missing side falls back to baseMap.
//
// If cond is nil or has kind "unknown", the solver cannot reason about ite,
// so the function falls back to last-writer-wins: the else-branch overwrites
// the base, and the then-branch supplements for names not in else.
func mergeFlowMaps(cond *symExpr, thenMap, elseMap, baseMap flowMap) flowMap {
	// Unknown condition: fall back to last-writer-wins.
	if cond == nil || cond.Kind == "unknown" {
		result := snapshot(baseMap)
		for k, v := range elseMap {
			result[k] = v
		}
		for k, v := range thenMap {
			if _, ok := result[k]; !ok {
				result[k] = v
			}
		}
		return result
	}

	// Collect all variable names present in either branch.
	allVars := make(map[string]struct{}, len(thenMap)+len(elseMap))
	for k := range thenMap {
		allVars[k] = struct{}{}
	}
	for k := range elseMap {
		allVars[k] = struct{}{}
	}

	result := make(flowMap, len(allVars))
	for name := range allVars {
		thenVal := thenMap[name]
		elseVal := elseMap[name]
		preVal := baseMap[name]

		// Pointer equality — both branches resolved to the same node.
		if thenVal == elseVal {
			if thenVal != nil {
				result[name] = thenVal
			}
			continue
		}

		// Structural equality via JSON serialisation (mirrors TS JSON.stringify).
		if thenVal != nil && elseVal != nil && symExprsEqual(thenVal, elseVal) {
			result[name] = thenVal
			continue
		}

		// Variable introduced inside exactly one branch and not pre-existing.
		if preVal == nil && (thenVal == nil || elseVal == nil) {
			val := thenVal
			if val == nil {
				val = elseVal
			}
			if val != nil {
				result[name] = val
			}
			continue
		}

		// Divergent values: produce ite, falling back to the pre-if value for
		// the side that did not define the variable.
		thenExpr := thenVal
		if thenExpr == nil {
			thenExpr = preVal
		}
		elseExpr := elseVal
		if elseExpr == nil {
			elseExpr = preVal
		}

		switch {
		case thenExpr != nil && elseExpr != nil:
			result[name] = &symExpr{
				Kind:      "ite",
				Condition: cond,
				ThenExpr:  thenExpr,
				ElseExpr:  elseExpr,
			}
		case thenExpr != nil:
			result[name] = thenExpr
		case elseExpr != nil:
			result[name] = elseExpr
		}
	}
	return result
}

// symExprsEqual reports whether a and b have the same JSON representation.
// This is a structural equality check that mirrors TS's JSON.stringify
// comparison used in mergeFlowMaps.
func symExprsEqual(a, b *symExpr) bool {
	if a == b {
		return true
	}
	aj, err := json.Marshal(a)
	if err != nil {
		return false
	}
	bj, err := json.Marshal(b)
	if err != nil {
		return false
	}
	return string(aj) == string(bj)
}
