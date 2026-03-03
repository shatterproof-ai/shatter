package instrument

import (
	"bytes"
	"go/ast"
	"go/printer"
	"go/token"
	"strconv"
)

// symExpr is a symbolic expression representing a constraint on inputs.
// This mirrors protocol.SymExpr but is defined locally to avoid import cycles.
type symExpr struct {
	Kind     string    `json:"kind"`
	Name     string    `json:"name,omitempty"`
	Path     []string  `json:"path"`
	Type     string    `json:"type,omitempty"`
	Value    any       `json:"value,omitempty"`
	Op       string    `json:"op,omitempty"`
	Left     *symExpr  `json:"left,omitempty"`
	Right    *symExpr  `json:"right,omitempty"`
	Operand  *symExpr  `json:"operand,omitempty"`
	Receiver *symExpr  `json:"receiver,omitempty"`
	Args     []symExpr `json:"args,omitempty"`
}

// symConstraint is either an expression constraint or an unknown hint.
// This mirrors protocol.SymConstraint but is defined locally to avoid import cycles.
type symConstraint struct {
	Kind string   `json:"kind"`
	Expr *symExpr `json:"expr,omitempty"`
	Hint string   `json:"hint,omitempty"`
}

// exprToSymExpr converts a Go AST expression into a symExpr tree.
// params is the set of function parameter names for identifying symbolic variables.
func exprToSymExpr(expr ast.Expr, params map[string]bool) *symExpr {
	switch e := expr.(type) {
	case *ast.ParenExpr:
		return exprToSymExpr(e.X, params)

	case *ast.Ident:
		if params[e.Name] {
			return &symExpr{Kind: "param", Name: e.Name, Path: []string{}}
		}
		if e.Name == "true" {
			return &symExpr{Kind: "const", Type: "bool", Value: true}
		}
		if e.Name == "false" {
			return &symExpr{Kind: "const", Type: "bool", Value: false}
		}
		if e.Name == "nil" {
			return &symExpr{Kind: "const", Type: "null", Value: nil}
		}
		return &symExpr{Kind: "unknown"}

	case *ast.BasicLit:
		return basicLitToSymExpr(e)

	case *ast.BinaryExpr:
		op := tokenToOp(e.Op)
		if op == "" {
			return &symExpr{Kind: "unknown"}
		}
		return &symExpr{
			Kind:  "bin_op",
			Op:    op,
			Left:  exprToSymExpr(e.X, params),
			Right: exprToSymExpr(e.Y, params),
		}

	case *ast.UnaryExpr:
		var op string
		switch e.Op {
		case token.NOT:
			op = "not"
		case token.SUB:
			op = "neg"
		case token.XOR:
			op = "bitwise_not"
		default:
			return &symExpr{Kind: "unknown"}
		}
		return &symExpr{
			Kind:    "un_op",
			Op:      op,
			Operand: exprToSymExpr(e.X, params),
		}

	case *ast.SelectorExpr:
		if ident, ok := e.X.(*ast.Ident); ok && params[ident.Name] {
			return &symExpr{
				Kind: "param",
				Name: ident.Name,
				Path: []string{e.Sel.Name},
			}
		}
		return &symExpr{Kind: "unknown"}

	case *ast.CallExpr:
		return callExprToSymExpr(e, params)

	default:
		return &symExpr{Kind: "unknown"}
	}
}

func basicLitToSymExpr(lit *ast.BasicLit) *symExpr {
	switch lit.Kind {
	case token.INT:
		n, err := strconv.ParseInt(lit.Value, 0, 64)
		if err != nil {
			return &symExpr{Kind: "unknown"}
		}
		return &symExpr{Kind: "const", Type: "int", Value: n}
	case token.FLOAT:
		f, err := strconv.ParseFloat(lit.Value, 64)
		if err != nil {
			return &symExpr{Kind: "unknown"}
		}
		return &symExpr{Kind: "const", Type: "float", Value: f}
	case token.STRING:
		// Strip quotes
		s, err := strconv.Unquote(lit.Value)
		if err != nil {
			s = lit.Value
		}
		return &symExpr{Kind: "const", Type: "str", Value: s}
	case token.CHAR:
		s, err := strconv.Unquote(lit.Value)
		if err != nil {
			s = lit.Value
		}
		return &symExpr{Kind: "const", Type: "str", Value: s}
	default:
		return &symExpr{Kind: "unknown"}
	}
}

func callExprToSymExpr(call *ast.CallExpr, params map[string]bool) *symExpr {
	var name string
	switch fn := call.Fun.(type) {
	case *ast.Ident:
		name = fn.Name
	case *ast.SelectorExpr:
		if x, ok := fn.X.(*ast.Ident); ok {
			name = x.Name + "." + fn.Sel.Name
		} else {
			return &symExpr{Kind: "unknown"}
		}
	default:
		return &symExpr{Kind: "unknown"}
	}

	args := make([]symExpr, len(call.Args))
	for i, arg := range call.Args {
		args[i] = *exprToSymExpr(arg, params)
	}
	return &symExpr{Kind: "call", Name: name, Args: args}
}

func tokenToOp(tok token.Token) string {
	switch tok {
	case token.GTR:
		return "gt"
	case token.LSS:
		return "lt"
	case token.GEQ:
		return "ge"
	case token.LEQ:
		return "le"
	case token.EQL:
		return "eq"
	case token.NEQ:
		return "ne"
	case token.LAND:
		return "and"
	case token.LOR:
		return "or"
	case token.ADD:
		return "add"
	case token.SUB:
		return "sub"
	case token.MUL:
		return "mul"
	case token.QUO:
		return "div"
	case token.REM:
		return "mod"
	case token.NOT:
		return "not"
	case token.AND:
		return "bitwise_and"
	case token.OR:
		return "bitwise_or"
	case token.XOR:
		return "bitwise_xor"
	case token.SHL:
		return "shl"
	case token.SHR:
		return "shr"
	case token.AND_NOT:
		return "bit_clear"
	default:
		return ""
	}
}

// extractConstraint converts a Go AST expression into a symConstraint.
// If the expression can be represented symbolically, returns kind "expr".
// Otherwise returns kind "unknown" with the source text as a hint.
func extractConstraint(fset *token.FileSet, expr ast.Expr, params map[string]bool) symConstraint {
	sym := exprToSymExpr(expr, params)
	if sym.Kind != "unknown" {
		return symConstraint{Kind: "expr", Expr: sym}
	}
	return symConstraint{Kind: "unknown", Hint: exprToSource(fset, expr)}
}

func exprToSource(fset *token.FileSet, expr ast.Expr) string {
	var buf bytes.Buffer
	if err := printer.Fprint(&buf, fset, expr); err != nil {
		return "<unprintable>"
	}
	return buf.String()
}
