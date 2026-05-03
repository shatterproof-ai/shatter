package instrument

import (
	"encoding/json"
	"fmt"
	"go/ast"
	"go/token"
	"strconv"
)

// transformFile walks the AST and inserts line recording, branch recording,
// and scope event calls. If funcName is non-nil, only the named function is
// instrumented. Returns the number of branches instrumented.
func transformFile(fset *token.FileSet, file *ast.File, funcName *string) int {
	branchID := 0
	loopID := 0
	callSiteID := 0
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok || fn.Body == nil {
			continue
		}
		if funcName != nil && fn.Name.Name != *funcName {
			continue
		}
		params := buildParamSet(fn)

		// Inject call_enter/call_exit for the top-level function.
		id := callSiteID
		callSiteID++
		fn.Body.List = append([]ast.Stmt{
			makeScopeRecordStmt("call_enter", id),
			makeDeferScopeRecordStmt("call_exit", id),
		}, fn.Body.List...)

		transformBlock(fset, fn.Body, params, &branchID, &loopID, &callSiteID)
	}
	return branchID
}

// buildParamSet extracts parameter names from a function declaration.
func buildParamSet(fn *ast.FuncDecl) map[string]bool {
	params := make(map[string]bool)
	if fn.Type.Params == nil {
		return params
	}
	for _, field := range fn.Type.Params.List {
		for _, name := range field.Names {
			params[name.Name] = true
		}
	}
	return params
}

// transformBlock instruments a block statement by inserting line recording
// before each statement, wrapping branch conditions, and adding scope markers.
func transformBlock(fset *token.FileSet, block *ast.BlockStmt, params map[string]bool, branchID, loopID, callSiteID *int) {
	if block == nil {
		return
	}
	block.List = transformStmtList(fset, block.List, params, branchID, loopID, callSiteID, block)
}

// transformStmtList applies the same line/branch/scope instrumentation
// transformBlock applies to a *ast.BlockStmt, but operates on a raw
// []ast.Stmt — the shape carried by *ast.CaseClause.Body and
// *ast.CommClause.Body. Sharing this helper guarantees that statements
// inside switch case bodies receive the same line records as statements
// inside ordinary block bodies. Without this sharing, return statements
// (and other statements) inside switch cases never appeared in
// lines_executed, which is the root cause of str-qo1.12.
//
// enclosingBlock may be nil when the statements are not lexically inside a
// *ast.BlockStmt (e.g. switch case bodies). In that case, the
// reassignment-after-closure analysis used by instrumentFuncLits is
// skipped, but call_enter/call_exit recording is still emitted.
func transformStmtList(
	fset *token.FileSet,
	stmts []ast.Stmt,
	params map[string]bool,
	branchID, loopID, callSiteID *int,
	enclosingBlock *ast.BlockStmt,
) []ast.Stmt {
	var newList []ast.Stmt
	for _, stmt := range stmts {
		line := fset.Position(stmt.Pos()).Line
		newList = append(newList, makeLineRecordCall(line))
		switch s := stmt.(type) {
		case *ast.IfStmt:
			transformIfStmt(fset, s, params, branchID, loopID, callSiteID)
		case *ast.SwitchStmt:
			transformSwitchStmt(fset, s, params, branchID, loopID, callSiteID)
		case *ast.ForStmt:
			transformForStmt(fset, s, params, branchID, loopID, callSiteID)
		case *ast.RangeStmt:
			transformRangeStmt(fset, s, params, branchID, loopID, callSiteID)
		}
		// Instrument function literals in expressions (callbacks).
		instrumentFuncLits(fset, stmt, params, branchID, loopID, callSiteID, enclosingBlock)
		newList = append(newList, stmt)
	}
	return newList
}

func transformIfStmt(fset *token.FileSet, s *ast.IfStmt, params map[string]bool, branchID, loopID, callSiteID *int) {
	if s.Cond != nil {
		line := fset.Position(s.Cond.Pos()).Line
		s.Cond = wrapCondition(fset, s.Cond, line, params, branchID)
	}
	transformBlock(fset, s.Body, params, branchID, loopID, callSiteID)
	if s.Else != nil {
		switch e := s.Else.(type) {
		case *ast.BlockStmt:
			transformBlock(fset, e, params, branchID, loopID, callSiteID)
		case *ast.IfStmt:
			transformIfStmt(fset, e, params, branchID, loopID, callSiteID)
		}
	}
}

func transformSwitchStmt(fset *token.FileSet, s *ast.SwitchStmt, params map[string]bool, branchID, loopID, callSiteID *int) {
	for _, stmt := range s.Body.List {
		cc, ok := stmt.(*ast.CaseClause)
		if !ok {
			continue
		}
		line := fset.Position(cc.Pos()).Line
		constraint := constraintForCase(fset, s.Tag, cc, params)
		id := *branchID
		*branchID++
		recordCall := makeBranchRecordStmt(id, line, constraint)
		// Instrument the statements inside the case body so each one
		// emits a line record (and any nested control flow recurses).
		// Without this, return statements like `return "go"` in a switch
		// case body never appeared in lines_executed — the root cause of
		// str-qo1.12. The branch record stays first so the case-entry
		// signal precedes any per-statement line record.
		instrumented := transformStmtList(fset, cc.Body, params, branchID, loopID, callSiteID, nil)
		cc.Body = append([]ast.Stmt{recordCall}, instrumented...)
	}
}

func constraintForCase(fset *token.FileSet, tag ast.Expr, cc *ast.CaseClause, params map[string]bool) string {
	if cc.List == nil {
		// default case
		c := extractConstraint(fset, &ast.Ident{Name: "true"}, params)
		data, _ := json.Marshal(c)
		return string(data)
	}
	// Build a disjunction over every case literal so the solver can target
	// each alternative in a multi-literal clause (`case A, B:`). Without
	// this, only the first literal's equality reaches the solver and any
	// path that needs `tag == B` is unreachable. (str-5jen)
	expr := caseClauseExpr(tag, cc.List[0])
	for _, lit := range cc.List[1:] {
		expr = &ast.BinaryExpr{
			X:  expr,
			Op: token.LOR,
			Y:  caseClauseExpr(tag, lit),
		}
	}
	c := extractConstraint(fset, expr, params)
	data, _ := json.Marshal(c)
	return string(data)
}

// caseClauseExpr returns the AST expression that must hold for a single
// case-clause literal: `tag == lit` for value switches, or `lit` itself
// for tagless boolean switches (`switch { case x > 0: }`).
func caseClauseExpr(tag, lit ast.Expr) ast.Expr {
	if tag == nil {
		return lit
	}
	return &ast.BinaryExpr{X: tag, Op: token.EQL, Y: lit}
}

func transformForStmt(fset *token.FileSet, s *ast.ForStmt, params map[string]bool, branchID, loopID, callSiteID *int) {
	if s.Cond != nil {
		line := fset.Position(s.Cond.Pos()).Line
		s.Cond = wrapCondition(fset, s.Cond, line, params, branchID)
	}
	// Inject loop scope markers inside the loop body (per-iteration).
	id := *loopID
	*loopID++
	if s.Body != nil {
		s.Body.List = append(
			[]ast.Stmt{makeScopeRecordStmt("loop_enter", id)},
			append(s.Body.List, makeScopeRecordStmt("loop_exit", id))...,
		)
	}
	transformBlock(fset, s.Body, params, branchID, loopID, callSiteID)
}

func transformRangeStmt(fset *token.FileSet, s *ast.RangeStmt, _ /*params*/ map[string]bool, branchID, loopID, _ /*callSiteID*/ *int) {
	if s.Body != nil {
		line := fset.Position(s.Pos()).Line
		unknownConstraint := `{"kind":"unknown","hint":"range loop"}`
		bid := *branchID
		*branchID++
		recordCall := makeBranchRecordStmt(bid, line, unknownConstraint)

		// Inject loop scope markers (per-iteration).
		lid := *loopID
		*loopID++
		s.Body.List = append(
			[]ast.Stmt{makeScopeRecordStmt("loop_enter", lid), recordCall},
			append(s.Body.List, makeScopeRecordStmt("loop_exit", lid))...,
		)
	}
}

// wrapCondition wraps a branch condition with the appropriate recording call.
// When MC/DC mode is enabled and the condition is a pure && or || chain with
// at most maxMcdcConditions leaves, generates the MC/DC recording call.
// Otherwise falls back to the standard __shatter_record_branch call.
func wrapCondition(fset *token.FileSet, cond ast.Expr, line int, params map[string]bool, branchID *int) ast.Expr {
	id := *branchID
	*branchID++

	if isMcdcEnabled() {
		if flattened := flattenConditionsAST(cond, fset, params); flattened != nil {
			return buildMcdcBranchCall(id, line, flattened)
		}
		// Mixed operators or single condition: warn is not needed at instrumentation
		// time (the recorder emits an empty conditions array, which the core
		// interprets as a non-compound decision).
	}

	constraint := extractConstraint(fset, cond, params)
	return makeBranchRecordCall(id, line, cond, constraint)
}

// makeLineRecordCall creates: __shatter_record_line(LINE)
func makeLineRecordCall(line int) ast.Stmt {
	return &ast.ExprStmt{
		X: &ast.CallExpr{
			Fun:  ast.NewIdent("__shatter_record_line"),
			Args: []ast.Expr{intLit(line)},
		},
	}
}

// makeBranchRecordCall creates: __shatter_record_branch(ID, LINE, COND, constraintJSON)
// This replaces the condition expression so the result passes through.
func makeBranchRecordCall(branchID, line int, cond ast.Expr, constraint symConstraint) ast.Expr {
	constraintJSON, _ := json.Marshal(constraint)
	return &ast.CallExpr{
		Fun: ast.NewIdent("__shatter_record_branch"),
		Args: []ast.Expr{
			intLit(branchID),
			intLit(line),
			cond,
			stringLit(string(constraintJSON)),
		},
	}
}

// makeBranchRecordStmt creates a statement: __shatter_record_branch(ID, LINE, true, constraintJSON)
func makeBranchRecordStmt(branchID, line int, constraintJSON string) ast.Stmt {
	return &ast.ExprStmt{
		X: &ast.CallExpr{
			Fun: ast.NewIdent("__shatter_record_branch"),
			Args: []ast.Expr{
				intLit(branchID),
				intLit(line),
				ast.NewIdent("true"),
				stringLit(constraintJSON),
			},
		},
	}
}

func intLit(n int) *ast.BasicLit {
	return &ast.BasicLit{Kind: token.INT, Value: strconv.Itoa(n)}
}

func stringLit(s string) *ast.BasicLit {
	return &ast.BasicLit{Kind: token.STRING, Value: fmt.Sprintf("%q", s)}
}

// makeScopeRecordStmt creates: __shatter_record_scope(KIND, ID)
func makeScopeRecordStmt(kind string, id int) ast.Stmt {
	return &ast.ExprStmt{
		X: &ast.CallExpr{
			Fun: ast.NewIdent("__shatter_record_scope"),
			Args: []ast.Expr{
				stringLit(kind),
				intLit(id),
			},
		},
	}
}

// makeDeferScopeRecordStmt creates: defer __shatter_record_scope(KIND, ID)
func makeDeferScopeRecordStmt(kind string, id int) ast.Stmt {
	return &ast.DeferStmt{
		Call: &ast.CallExpr{
			Fun: ast.NewIdent("__shatter_record_scope"),
			Args: []ast.Expr{
				stringLit(kind),
				intLit(id),
			},
		},
	}
}

// instrumentFuncLits walks the statement's expression tree and instruments
// any function literals (closures) with call scope markers.
// enclosingBlock is used to check if captured outer params are reassigned after the closure.
func instrumentFuncLits(fset *token.FileSet, stmt ast.Stmt, params map[string]bool, branchID, loopID, callSiteID *int, enclosingBlock *ast.BlockStmt) {
	ast.Inspect(stmt, func(n ast.Node) bool {
		fl, ok := n.(*ast.FuncLit)
		if !ok || fl.Body == nil {
			return true
		}
		id := *callSiteID
		*callSiteID++
		fl.Body.List = append([]ast.Stmt{
			makeScopeRecordStmt("call_enter", id),
			makeDeferScopeRecordStmt("call_exit", id),
		}, fl.Body.List...)

		// Collect identifiers referenced inside the closure body.
		capturedIdents := collectIdentifiers(fl.Body)

		// Build param set from the FuncLit's own parameters.
		funcParams := make(map[string]bool)
		for k, v := range params {
			// Exclude outer params that are captured by the closure and
			// reassigned after the closure's position in the enclosing block.
			// Go closures capture by reference, so the symbolic link is unreliable.
			if capturedIdents[k] && enclosingBlock != nil && isReassignedAfter(k, enclosingBlock.List, fl.Pos()) {
				continue
			}
			funcParams[k] = v
		}
		if fl.Type.Params != nil {
			for _, field := range fl.Type.Params.List {
				for _, name := range field.Names {
					funcParams[name.Name] = true
				}
			}
		}
		transformBlock(fset, fl.Body, funcParams, branchID, loopID, callSiteID)
		return false // don't recurse into the body again
	})
}

// collectIdentifiers walks an AST node and returns all identifier names referenced.
// Skips parameter names of nested FuncLit nodes (they shadow, not capture).
func collectIdentifiers(node ast.Node) map[string]bool {
	idents := make(map[string]bool)
	ast.Inspect(node, func(n ast.Node) bool {
		switch x := n.(type) {
		case *ast.FuncLit:
			// Don't recurse into nested closures — their params shadow
			if x != node {
				return false
			}
		case *ast.Ident:
			idents[x.Name] = true
		}
		return true
	})
	return idents
}

// isReassignedAfter checks if varName is assigned in any statement in stmts
// that appears after the given position.
func isReassignedAfter(varName string, stmts []ast.Stmt, pos token.Pos) bool {
	for _, stmt := range stmts {
		if stmt.Pos() <= pos {
			continue
		}
		if hasAssignment(varName, stmt) {
			return true
		}
	}
	return false
}

// hasAssignment checks if a node contains an assignment to varName.
func hasAssignment(varName string, node ast.Node) bool {
	found := false
	ast.Inspect(node, func(n ast.Node) bool {
		if found {
			return false
		}
		// Skip nested function literals — they have their own scope
		if fl, ok := n.(*ast.FuncLit); ok && fl != node {
			return false
		}
		assign, ok := n.(*ast.AssignStmt)
		if !ok {
			return true
		}
		for _, lhs := range assign.Lhs {
			if ident, ok := lhs.(*ast.Ident); ok && ident.Name == varName {
				found = true
				return false
			}
		}
		return true
	})
	// Also check for increment/decrement statements
	if !found {
		ast.Inspect(node, func(n ast.Node) bool {
			if found {
				return false
			}
			inc, ok := n.(*ast.IncDecStmt)
			if !ok {
				return true
			}
			if ident, ok := inc.X.(*ast.Ident); ok && ident.Name == varName {
				found = true
				return false
			}
			return true
		})
	}
	return found
}
