package protocol

import (
	"encoding/json"
	"go/ast"
	"go/parser"
	"go/token"
	"os"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
)

func buildLoopBodyStatesFromAnalysis(analysis *FunctionAnalysis, scopeEvents []json.RawMessage) []instrument.LoopBodyState {
	if analysis == nil {
		return nil
	}
	states := buildLoopBodyStatesFromScopeEvents(analysis.Loops, scopeEvents)
	if len(states) == 0 || analysis.SourceFile == "" {
		return states
	}

	source, err := os.ReadFile(analysis.SourceFile)
	if err != nil {
		return states
	}
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, analysis.SourceFile, source, 0)
	if err != nil {
		return states
	}
	fn := findLoopSnapshotFunction(file, analysis.Name)
	if fn == nil || fn.Body == nil {
		return states
	}

	localsByLoopIteration := collectGoLoopSnapshotLocals(fset, fn, analysis.Loops, scopeEvents)
	for i := range states {
		key := loopIterationKey{loopID: states[i].LoopID, iteration: states[i].Iteration}
		if locals := localsByLoopIteration[key]; len(locals) > 0 {
			states[i].Locals = encodeLoopSnapshotLocals(locals)
		}
	}
	return states
}

type loopIterationKey struct {
	loopID    int
	iteration int
}

func findLoopSnapshotFunction(file *ast.File, name string) *ast.FuncDecl {
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if ok && fn.Name.Name == name {
			return fn
		}
	}
	return nil
}

func collectGoLoopSnapshotLocals(
	fset *token.FileSet,
	fn *ast.FuncDecl,
	loops []LoopInfo,
	scopeEvents []json.RawMessage,
) map[loopIterationKey]map[string]SymExpr {
	iterationCounts := countLoopSnapshotIterations(scopeEvents)
	if len(iterationCounts) == 0 {
		return nil
	}

	loopsByLine := make(map[int]LoopInfo, len(loops))
	for _, loop := range loops {
		loopsByLine[loop.Line] = loop
	}

	params := make(map[string]bool)
	for _, field := range fn.Type.Params.List {
		for _, name := range field.Names {
			params[name.Name] = true
		}
	}

	flow := make(map[string]*SymExpr)
	locals := make(map[loopIterationKey]map[string]SymExpr)
	visitGoLoopSnapshotStatements(fset, fn.Body.List, loopsByLine, iterationCounts, params, flow, locals)
	return locals
}

func countLoopSnapshotIterations(scopeEvents []json.RawMessage) map[int]int {
	counts := make(map[int]int)
	for _, raw := range scopeEvents {
		loopID, ok := decodeLoopSnapshotEnterID(raw)
		if ok {
			counts[loopID]++
		}
	}
	return counts
}

func decodeLoopSnapshotEnterID(raw json.RawMessage) (int, bool) {
	var event struct {
		Type  string `json:"type"`
		Kind  string `json:"kind"`
		Event *struct {
			Kind   string `json:"kind"`
			LoopID *int   `json:"loop_id"`
		} `json:"event"`
	}
	if err := json.Unmarshal(raw, &event); err != nil {
		return 0, false
	}
	if event.Type != "scope" && event.Kind != "scope" {
		return 0, false
	}
	if event.Event == nil || event.Event.Kind != "loop_enter" || event.Event.LoopID == nil {
		return 0, false
	}
	return *event.Event.LoopID, true
}

func visitGoLoopSnapshotStatements(
	fset *token.FileSet,
	statements []ast.Stmt,
	loopsByLine map[int]LoopInfo,
	iterationCounts map[int]int,
	params map[string]bool,
	flow map[string]*SymExpr,
	locals map[loopIterationKey]map[string]SymExpr,
) {
	for _, stmt := range statements {
		switch s := stmt.(type) {
		case *ast.DeclStmt:
			visitGoLoopSnapshotDecl(s, params, flow)
		case *ast.AssignStmt:
			visitGoLoopSnapshotAssign(s, params, flow)
		case *ast.IncDecStmt:
			visitGoLoopSnapshotIncDec(s, flow)
		case *ast.ForStmt:
			visitGoLoopSnapshotFor(fset, s, loopsByLine, iterationCounts, params, flow, locals)
		case *ast.BlockStmt:
			visitGoLoopSnapshotStatements(fset, s.List, loopsByLine, iterationCounts, params, flow, locals)
		}
	}
}

func visitGoLoopSnapshotFor(
	fset *token.FileSet,
	stmt *ast.ForStmt,
	loopsByLine map[int]LoopInfo,
	iterationCounts map[int]int,
	params map[string]bool,
	flow map[string]*SymExpr,
	locals map[loopIterationKey]map[string]SymExpr,
) {
	if stmt.Init != nil {
		visitGoLoopSnapshotStatement(fset, stmt.Init, loopsByLine, iterationCounts, params, flow, locals)
	}

	line := fset.Position(stmt.For).Line
	loop, ok := loopsByLine[line]
	if !ok {
		visitGoLoopSnapshotStatements(fset, stmt.Body.List, loopsByLine, iterationCounts, params, flow, locals)
		return
	}

	tracked := collectGoLoopTrackedLocals(stmt.Body, loop)
	for iteration := 0; iteration < iterationCounts[loop.LoopID]; iteration++ {
		snapshot := make(map[string]SymExpr)
		for _, name := range tracked {
			expr := resolveGoLoopSnapshotName(name, params, flow)
			if expr != nil && expr.Kind != "unknown" {
				snapshot[name] = *expr
			}
		}
		if len(snapshot) > 0 {
			locals[loopIterationKey{loopID: loop.LoopID, iteration: iteration}] = snapshot
		}
		visitGoLoopSnapshotStatements(fset, stmt.Body.List, loopsByLine, iterationCounts, params, flow, locals)
		if stmt.Post != nil {
			visitGoLoopSnapshotStatement(fset, stmt.Post, loopsByLine, iterationCounts, params, flow, locals)
		}
	}
}

func visitGoLoopSnapshotStatement(
	fset *token.FileSet,
	stmt ast.Stmt,
	loopsByLine map[int]LoopInfo,
	iterationCounts map[int]int,
	params map[string]bool,
	flow map[string]*SymExpr,
	locals map[loopIterationKey]map[string]SymExpr,
) {
	switch s := stmt.(type) {
	case *ast.DeclStmt:
		visitGoLoopSnapshotDecl(s, params, flow)
	case *ast.AssignStmt:
		visitGoLoopSnapshotAssign(s, params, flow)
	case *ast.IncDecStmt:
		visitGoLoopSnapshotIncDec(s, flow)
	case *ast.ForStmt:
		visitGoLoopSnapshotFor(fset, s, loopsByLine, iterationCounts, params, flow, locals)
	case *ast.BlockStmt:
		visitGoLoopSnapshotStatements(fset, s.List, loopsByLine, iterationCounts, params, flow, locals)
	}
}

func visitGoLoopSnapshotDecl(stmt *ast.DeclStmt, params map[string]bool, flow map[string]*SymExpr) {
	decl, ok := stmt.Decl.(*ast.GenDecl)
	if !ok {
		return
	}
	for _, spec := range decl.Specs {
		valueSpec, ok := spec.(*ast.ValueSpec)
		if !ok {
			continue
		}
		for i, name := range valueSpec.Names {
			if i < len(valueSpec.Values) {
				flow[name.Name] = buildGoLoopSnapshotSymExpr(valueSpec.Values[i], params, flow)
			}
		}
	}
}

func visitGoLoopSnapshotAssign(stmt *ast.AssignStmt, params map[string]bool, flow map[string]*SymExpr) {
	for i, lhs := range stmt.Lhs {
		name, ok := lhs.(*ast.Ident)
		if !ok {
			continue
		}
		var rhs ast.Expr
		if i < len(stmt.Rhs) {
			rhs = stmt.Rhs[i]
		}
		switch stmt.Tok {
		case token.ASSIGN, token.DEFINE:
			flow[name.Name] = buildGoLoopSnapshotSymExpr(rhs, params, flow)
		case token.ADD_ASSIGN, token.SUB_ASSIGN, token.MUL_ASSIGN, token.QUO_ASSIGN:
			flow[name.Name] = &SymExpr{
				Kind:  "bin_op",
				Op:    assignTokenToLoopSnapshotOp(stmt.Tok),
				Left:  resolveGoLoopSnapshotName(name.Name, params, flow),
				Right: buildGoLoopSnapshotSymExpr(rhs, params, flow),
				Args:  []SymExpr{},
			}
		}
	}
}

func visitGoLoopSnapshotIncDec(stmt *ast.IncDecStmt, flow map[string]*SymExpr) {
	name, ok := stmt.X.(*ast.Ident)
	if !ok {
		return
	}
	op := "add"
	if stmt.Tok == token.DEC {
		op = "sub"
	}
	flow[name.Name] = &SymExpr{
		Kind:  "bin_op",
		Op:    op,
		Left:  flow[name.Name],
		Right: &SymExpr{Kind: "const", Type: "int", Value: int64(1), Args: []SymExpr{}},
		Args:  []SymExpr{},
	}
}

func collectGoLoopTrackedLocals(body *ast.BlockStmt, loop LoopInfo) []string {
	seen := make(map[string]bool)
	var names []string
	if loop.InductionVar != nil && loop.InductionVar.Name != "" {
		seen[loop.InductionVar.Name] = true
		names = append(names, loop.InductionVar.Name)
	}
	ast.Inspect(body, func(node ast.Node) bool {
		assign, ok := node.(*ast.AssignStmt)
		if !ok {
			return true
		}
		for _, lhs := range assign.Lhs {
			name, ok := lhs.(*ast.Ident)
			if ok && !seen[name.Name] {
				seen[name.Name] = true
				names = append(names, name.Name)
			}
		}
		return true
	})
	return names
}

func buildGoLoopSnapshotSymExpr(expr ast.Expr, params map[string]bool, flow map[string]*SymExpr) *SymExpr {
	if expr == nil {
		return &SymExpr{Kind: "unknown", Args: []SymExpr{}}
	}
	switch e := expr.(type) {
	case *ast.Ident:
		return resolveGoLoopSnapshotName(e.Name, params, flow)
	case *ast.BasicLit:
		return litSymExpr(e)
	case *ast.BinaryExpr:
		return &SymExpr{
			Kind:  "bin_op",
			Op:    tokenToOp(e.Op),
			Left:  buildGoLoopSnapshotSymExpr(e.X, params, flow),
			Right: buildGoLoopSnapshotSymExpr(e.Y, params, flow),
			Args:  []SymExpr{},
		}
	case *ast.UnaryExpr:
		if e.Op == token.SUB {
			return &SymExpr{
				Kind:    "un_op",
				Op:      "neg",
				Operand: buildGoLoopSnapshotSymExpr(e.X, params, flow),
				Args:    []SymExpr{},
			}
		}
	case *ast.ParenExpr:
		return buildGoLoopSnapshotSymExpr(e.X, params, flow)
	}
	return &SymExpr{Kind: "unknown", Args: []SymExpr{}}
}

func resolveGoLoopSnapshotName(name string, params map[string]bool, flow map[string]*SymExpr) *SymExpr {
	if params[name] {
		return &SymExpr{Kind: "param", Name: name, Path: []string{}, Args: []SymExpr{}}
	}
	if expr := flow[name]; expr != nil {
		return expr
	}
	return &SymExpr{Kind: "unknown", Args: []SymExpr{}}
}

func assignTokenToLoopSnapshotOp(tok token.Token) string {
	switch tok {
	case token.SUB_ASSIGN:
		return "sub"
	case token.MUL_ASSIGN:
		return "mul"
	case token.QUO_ASSIGN:
		return "div"
	default:
		return "add"
	}
}

func encodeLoopSnapshotLocals(locals map[string]SymExpr) map[string]json.RawMessage {
	encoded := make(map[string]json.RawMessage, len(locals))
	for name, expr := range locals {
		raw, err := json.Marshal(expr)
		if err == nil {
			encoded[name] = raw
		}
	}
	return encoded
}
