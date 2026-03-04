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
	var newList []ast.Stmt
	for _, stmt := range block.List {
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
		instrumentFuncLits(fset, stmt, params, branchID, loopID, callSiteID)
		newList = append(newList, stmt)
	}
	block.List = newList
}

func transformIfStmt(fset *token.FileSet, s *ast.IfStmt, params map[string]bool, branchID, loopID, callSiteID *int) {
	if s.Cond != nil {
		line := fset.Position(s.Cond.Pos()).Line
		constraint := extractConstraint(fset, s.Cond, params)
		s.Cond = makeBranchRecordCall(*branchID, line, s.Cond, constraint)
		*branchID++
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
		cc.Body = append([]ast.Stmt{recordCall}, cc.Body...)
	}
}

func constraintForCase(fset *token.FileSet, tag ast.Expr, cc *ast.CaseClause, params map[string]bool) string {
	if cc.List == nil {
		// default case
		c := extractConstraint(fset, &ast.Ident{Name: "true"}, params)
		data, _ := json.Marshal(c)
		return string(data)
	}
	if len(cc.List) == 1 && tag != nil {
		// single case value: create an equality constraint
		eq := &ast.BinaryExpr{X: tag, Op: token.EQL, Y: cc.List[0]}
		c := extractConstraint(fset, eq, params)
		data, _ := json.Marshal(c)
		return string(data)
	}
	c := extractConstraint(fset, cc.List[0], params)
	data, _ := json.Marshal(c)
	return string(data)
}

func transformForStmt(fset *token.FileSet, s *ast.ForStmt, params map[string]bool, branchID, loopID, callSiteID *int) {
	if s.Cond != nil {
		line := fset.Position(s.Cond.Pos()).Line
		constraint := extractConstraint(fset, s.Cond, params)
		s.Cond = makeBranchRecordCall(*branchID, line, s.Cond, constraint)
		*branchID++
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

func transformRangeStmt(fset *token.FileSet, s *ast.RangeStmt, params map[string]bool, branchID, loopID, callSiteID *int) {
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
func instrumentFuncLits(fset *token.FileSet, stmt ast.Stmt, params map[string]bool, branchID, loopID, callSiteID *int) {
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

		// Build param set from the FuncLit's own parameters.
		funcParams := make(map[string]bool)
		for k, v := range params {
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
