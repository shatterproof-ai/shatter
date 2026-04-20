package instrument

import (
	"encoding/json"
	"fmt"
	"go/ast"
	"go/token"
)

// maxMcdcConditions is the maximum number of leaf conditions per decision.
// Decisions exceeding this cap are treated as single (branch-only) decisions.
const maxMcdcConditions = 16

// mcdcOperator distinguishes pure && chains from pure || chains.
type mcdcOperator string

const (
	mcdcAnd mcdcOperator = "and"
	mcdcOr  mcdcOperator = "or"
)

// mcdcLeaf is one atomic condition extracted from a compound decision.
type mcdcLeaf struct {
	// expr is the original AST expression for this condition.
	expr ast.Expr
	// constraint is the symbolic constraint for this condition.
	constraint symConstraint
}

// mcdcFlattened holds the result of decomposing a pure && or || chain.
type mcdcFlattened struct {
	operator mcdcOperator
	leaves   []mcdcLeaf
}

// flattenConditionsAST decomposes a pure && or || expression tree into its
// leaf conditions. Returns nil when:
//   - the expression is not a BinaryExpr with LAND or LOR at the top level
//   - the top-level operator mixes && and || (mixed-operator decisions)
//   - the chain exceeds maxMcdcConditions leaves
//
// For pure chains, recursion collects all leaves in left-to-right source order.
func flattenConditionsAST(expr ast.Expr, fset *token.FileSet, params map[string]bool) *mcdcFlattened {
	bin, ok := expr.(*ast.BinaryExpr)
	if !ok {
		return nil
	}
	if bin.Op != token.LAND && bin.Op != token.LOR {
		return nil
	}

	var op mcdcOperator
	if bin.Op == token.LAND {
		op = mcdcAnd
	} else {
		op = mcdcOr
	}

	leaves := collectLeaves(bin, op, fset, params)
	if leaves == nil {
		// mixed operators detected
		return nil
	}
	if len(leaves) > maxMcdcConditions {
		return nil
	}
	return &mcdcFlattened{operator: op, leaves: leaves}
}

// collectLeaves recursively collects leaves from a pure && or || chain.
// Returns nil if a mixed-operator node is encountered at any level.
func collectLeaves(expr ast.Expr, op mcdcOperator, fset *token.FileSet, params map[string]bool) []mcdcLeaf {
	// Unwrap parentheses
	if paren, ok := expr.(*ast.ParenExpr); ok {
		return collectLeaves(paren.X, op, fset, params)
	}

	bin, ok := expr.(*ast.BinaryExpr)
	if !ok {
		// Atomic expression — this is a leaf.
		c := extractConstraint(fset, expr, params)
		return []mcdcLeaf{{expr: expr, constraint: c}}
	}

	var nodeOp mcdcOperator
	switch bin.Op {
	case token.LAND:
		nodeOp = mcdcAnd
	case token.LOR:
		nodeOp = mcdcOr
	default:
		// Non-boolean binary op (e.g., +, ==) — this is a leaf.
		c := extractConstraint(fset, expr, params)
		return []mcdcLeaf{{expr: expr, constraint: c}}
	}

	if nodeOp != op {
		// Mixed operators — abort decomposition.
		return nil
	}

	left := collectLeaves(bin.X, op, fset, params)
	if left == nil {
		return nil
	}
	right := collectLeaves(bin.Y, op, fset, params)
	if right == nil {
		return nil
	}
	return append(left, right...)
}

// buildMcdcBranchCall replaces a compound condition expression with a
// sequence that records per-condition outcomes.
//
// For a pure && chain `A && B && C`, it generates (as a statement + expression):
//
//	__shatter_mcdc_eval_0 := __shatter_mcdc_record(branchID, line, "and",
//	    []string{constraintA, constraintB, constraintC},
//	    func() bool { return !!(A) },
//	    func() bool { return !!(B) },
//	    func() bool { return !!(C) },
//	)
//
// The branch condition is then replaced by a call to __shatter_record_branch_mcdc
// which records the decision and returns the bool.
//
// Because Go AST instrumentation works by replacing expressions (not inserting
// statements before them), we use a helper approach: we assign the full mcdc
// call into a zero-argument closure call so the condition expression becomes
// a single expression:
//
//	func() bool {
//	    __shatter_mcdc_0 := __shatter_mcdc_record(...)
//	    return __shatter_record_branch_mcdc(branchID, line, __shatter_mcdc_0.decision,
//	                                        constraintJSON, __shatter_mcdc_0.conditions_json)
//	}()
func buildMcdcBranchCall(branchID, line int, flattened *mcdcFlattened) ast.Expr {
	// Build constraint JSON strings for each leaf.
	leafConstraintJSONs := make([]string, len(flattened.leaves))
	for i, leaf := range flattened.leaves {
		data, _ := json.Marshal(leaf.constraint)
		leafConstraintJSONs[i] = string(data)
	}

	// Build the argument list for __shatter_mcdc_record:
	// __shatter_mcdc_record(branchID, line int, op string, constraints []string,
	//                       thunks ...func() bool) __shatterMcdcResult
	args := []ast.Expr{
		intLit(branchID),
		intLit(line),
		stringLit(string(flattened.operator)),
	}

	// Constraints as a composite literal []string{...}
	constraintElems := make([]ast.Expr, len(leafConstraintJSONs))
	for i, cj := range leafConstraintJSONs {
		constraintElems[i] = stringLit(cj)
	}
	args = append(args, &ast.CompositeLit{
		Type: &ast.ArrayType{Elt: ast.NewIdent("string")},
		Elts: constraintElems,
	})

	// Each leaf becomes a thunk func() bool { return !!(expr) }
	for _, leaf := range flattened.leaves {
		thunk := makeThunk(leaf.expr)
		args = append(args, thunk)
	}

	// The mcdc variable name is local to the closure we generate.
	mcdcVarName := fmt.Sprintf("__shatter_mcdc_%d", branchID)

	// Build the inner IIFE:
	// func() bool {
	//   __shatter_mcdc_N := __shatter_mcdc_record(...)
	//   return __shatter_record_branch_mcdc(branchID, line, __shatter_mcdc_N)
	// }()
	mcdcCall := &ast.CallExpr{
		Fun:  ast.NewIdent("__shatter_mcdc_record"),
		Args: args,
	}

	// __shatter_mcdc_N := __shatter_mcdc_record(...)
	assignStmt := &ast.AssignStmt{
		Lhs: []ast.Expr{ast.NewIdent(mcdcVarName)},
		Tok: token.DEFINE,
		Rhs: []ast.Expr{mcdcCall},
	}

	// __shatter_record_branch_mcdc(branchID, line, __shatter_mcdc_N)
	branchCall := &ast.CallExpr{
		Fun: ast.NewIdent("__shatter_record_branch_mcdc"),
		Args: []ast.Expr{
			intLit(branchID),
			intLit(line),
			ast.NewIdent(mcdcVarName),
		},
	}

	returnStmt := &ast.ReturnStmt{
		Results: []ast.Expr{branchCall},
	}

	// Wrap in IIFE: func() bool { ... }()
	iife := &ast.CallExpr{
		Fun: &ast.FuncLit{
			Type: &ast.FuncType{
				Results: &ast.FieldList{
					List: []*ast.Field{
						{Type: ast.NewIdent("bool")},
					},
				},
			},
			Body: &ast.BlockStmt{
				List: []ast.Stmt{assignStmt, returnStmt},
			},
		},
	}
	return iife
}

// makeThunk wraps an expression as a zero-arg bool-returning function literal:
//
//	func() bool { return !!(expr) }
//
// The double-negation (!!) converts any truthy expression to a bool.
// In Go, `!!x` is only valid for bool x. For non-bool conditions we need
// a single `!` applied to the result of another `!`, but Go already requires
// the condition to be bool in an if/for statement. So we can use the expression
// directly (it's already a bool by Go's type rules).
func makeThunk(expr ast.Expr) ast.Expr {
	return &ast.FuncLit{
		Type: &ast.FuncType{
			Results: &ast.FieldList{
				List: []*ast.Field{
					{Type: ast.NewIdent("bool")},
				},
			},
		},
		Body: &ast.BlockStmt{
			List: []ast.Stmt{
				&ast.ReturnStmt{
					Results: []ast.Expr{expr},
				},
			},
		},
	}
}
