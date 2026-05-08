package instrument

import (
	"go/ast"
	"go/token"
)

// walkStmtsForFlow processes a slice of statements in declaration order,
// maintaining a flow map that tracks the symbolic value of local variables.
//
// It handles:
//   - *ast.AssignStmt (= and :=): rebuilds each RHS via exprToSymExprWithFlow
//     against the current state and updates the LHS name in the flow map.
//   - *ast.DeclStmt with var declarations: seeds named variables with their
//     initializer's symbolic value.
//   - *ast.IfStmt: snapshots the map before the condition, walks then/else
//     branches independently, then merges with mergeFlowMaps to produce ite
//     nodes where branches diverge.
//
// The input fm is not mutated; a fresh copy is made on entry and the updated
// copy is returned.
func walkStmtsForFlow(stmts []ast.Stmt, params map[string]bool, fm flowMap) flowMap {
	cur := snapshot(fm)
	for _, stmt := range stmts {
		cur = applyStmtToFlow(stmt, params, cur)
	}
	return cur
}

// applyStmtToFlow applies a single statement's data-flow effects to cur,
// returning the (possibly new) flow map state. Statements that have no
// data-flow relevance (returns, expressions without assignments, etc.) return
// cur unchanged.
func applyStmtToFlow(stmt ast.Stmt, params map[string]bool, cur flowMap) flowMap {
	switch s := stmt.(type) {
	case *ast.AssignStmt:
		return applyAssignToFlow(s, params, cur)
	case *ast.DeclStmt:
		return applyDeclToFlow(s, params, cur)
	case *ast.IfStmt:
		return applyIfToFlow(s, params, cur)
	case *ast.BlockStmt:
		if s != nil {
			return walkStmtsForFlow(s.List, params, cur)
		}
	}
	return cur
}

// applyAssignToFlow handles an *ast.AssignStmt (token.ASSIGN or token.DEFINE).
//
// Element-wise case (len(Lhs) == len(Rhs)): each RHS is evaluated with
// exprToSymExprWithFlow against the pre-assignment state so that
// simultaneous tuple assignments like "a, b = b, a" use the pre-swap values.
//
// Multi-return case (len(Rhs) == 1): we cannot decompose individual results
// without type information, so each LHS identifier is set to "unknown".
func applyAssignToFlow(s *ast.AssignStmt, params map[string]bool, cur flowMap) flowMap {
	out := snapshot(cur)

	switch {
	case len(s.Lhs) == len(s.Rhs):
		// Evaluate all RHS values against the pre-assignment state (cur, not out)
		// so simultaneous swaps resolve correctly.
		vals := make([]*symExpr, len(s.Rhs))
		for i, rhs := range s.Rhs {
			vals[i] = exprToSymExprWithFlow(rhs, params, cur)
		}
		for i, lhs := range s.Lhs {
			name := flowLHSName(lhs)
			if name == "" {
				continue
			}
			out[name] = vals[i]
		}
	case len(s.Rhs) == 1:
		// Multi-return call: we can't track individual results.
		for _, lhs := range s.Lhs {
			name := flowLHSName(lhs)
			if name == "" {
				continue
			}
			out[name] = &symExpr{Kind: "unknown"}
		}
	}
	return out
}

// applyDeclToFlow handles a *ast.DeclStmt for var declarations with initializers.
// Const declarations are not flow-tracked.
func applyDeclToFlow(ds *ast.DeclStmt, params map[string]bool, cur flowMap) flowMap {
	gd, ok := ds.Decl.(*ast.GenDecl)
	if !ok || gd.Tok != token.VAR {
		return cur
	}
	out := snapshot(cur)
	for _, spec := range gd.Specs {
		vs, ok := spec.(*ast.ValueSpec)
		if !ok || len(vs.Values) == 0 {
			continue
		}
		if len(vs.Names) == len(vs.Values) {
			for i, nameIdent := range vs.Names {
				if nameIdent.Name == "_" {
					continue
				}
				sym := exprToSymExprWithFlow(vs.Values[i], params, cur)
				out[nameIdent.Name] = sym
			}
		}
	}
	return out
}

// applyIfToFlow implements the SSA phi-node snapshot/merge protocol for an
// *ast.IfStmt:
//
//  1. Apply the Init statement (if any) against the current state.
//  2. Evaluate the condition symbolically.
//  3. Snapshot the state as "before".
//  4. Walk the then-body independently.
//  5. Walk the else-body (or a copy of "before" if absent) independently.
//  6. Return mergeFlowMaps(cond, thenMap, elseMap, before).
func applyIfToFlow(s *ast.IfStmt, params map[string]bool, cur flowMap) flowMap {
	// Step 1: process optional init (e.g., "if err := foo(); err != nil").
	if s.Init != nil {
		cur = applyStmtToFlow(s.Init, params, cur)
	}

	// Step 2: evaluate condition.
	cond := exprToSymExprWithFlow(s.Cond, params, cur)

	// Step 3: snapshot state before branching.
	before := snapshot(cur)

	// Step 4: then-branch walks from cur (includes Init vars).
	thenMap := walkStmtsForFlow(s.Body.List, params, cur)

	// Step 5: else-branch also starts from cur (the pre-branch state).
	var elseMap flowMap
	if s.Else != nil {
		switch e := s.Else.(type) {
		case *ast.BlockStmt:
			elseMap = walkStmtsForFlow(e.List, params, cur)
		case *ast.IfStmt:
			// else-if chain: treat the chained if as a single statement.
			elseMap = applyIfToFlow(e, params, snapshot(cur))
		}
	}
	if elseMap == nil {
		// No else clause: the else state is the pre-if state.
		elseMap = snapshot(before)
	}

	// Step 6: merge.
	return mergeFlowMaps(cond, thenMap, elseMap, before)
}

// flowLHSName returns the identifier name from an assignment LHS expression.
// Returns "" for blank identifiers ("_"), composite index expressions, and
// other non-identifier LHS forms that cannot be tracked in the flow map.
func flowLHSName(lhs ast.Expr) string {
	ident, ok := lhs.(*ast.Ident)
	if !ok || ident.Name == "_" {
		return ""
	}
	return ident.Name
}
