package instrument

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/printer"
	"go/token"
	"os"
	"strings"

	"golang.org/x/tools/go/ast/astutil"
)

// MockSubstitution describes a single execute-time call-site replacement:
// every call to QualifiedFunction (source-qualified, e.g. "auth.GetAccount")
// is replaced by the Go source Expression. This is the runtime half of the
// hint_config_v1 `.shatter/config.yaml` `mocks` contract (str-c8djq).
type MockSubstitution struct {
	// QualifiedFunction is the source-level call spelling: the local package
	// qualifier followed by the function name, e.g. "auth.GetAccount". The
	// qualifier is whatever the target file imports the package as (its alias
	// or default name), because substitution matches AST selector syntax, not
	// import paths.
	QualifiedFunction string
	// Expression is the Go source expression pasted in place of the whole
	// call. It must reference only packages already imported by the target
	// file (call-site substitution does not add imports).
	Expression string
}

// MockSubstitutionsFromConfigs extracts the expression-bearing subset of mocks
// as MockSubstitution entries. Wire mocks (ReturnValues only, empty Expression)
// are ignored — they flow through the ShatterMock shim path instead.
func MockSubstitutionsFromConfigs(mocks []MockConfig) []MockSubstitution {
	var subs []MockSubstitution
	for _, m := range mocks {
		if strings.TrimSpace(m.Expression) == "" {
			continue
		}
		key := normalizeMockSymbol(m.Symbol)
		if key == "" {
			continue
		}
		subs = append(subs, MockSubstitution{
			QualifiedFunction: key,
			Expression:        m.Expression,
		})
	}
	return subs
}

// normalizeMockSymbol converts a mock symbol into the source-qualified
// "qualifier.Func" spelling used to match AST selector call sites.
//
// Config mocks already arrive dotted ("auth.GetAccount"). The legacy wire /
// discovery form uses a colon between module and export ("module:Export");
// the colon is normalized to a dot so both spellings match the same call
// syntax. Only the final qualifier segment is retained because a
// package-qualified call in Go source is always a single-identifier selector
// (import path segments never appear at the call site).
func normalizeMockSymbol(symbol string) string {
	symbol = strings.TrimSpace(symbol)
	if symbol == "" {
		return ""
	}
	// Fold "module:Export" into "module.Export".
	if idx := strings.LastIndex(symbol, ":"); idx >= 0 {
		symbol = symbol[:idx] + "." + symbol[idx+1:]
	}
	// Reduce a dotted path to its final "qualifier.Func" pair. The qualifier
	// is the last path segment before the function name; anything earlier is
	// an import-path prefix that never appears at the source call site.
	parts := strings.Split(symbol, ".")
	if len(parts) < 2 {
		return ""
	}
	qualifier := parts[len(parts)-2]
	fn := parts[len(parts)-1]
	// A colon-form import path may leave a slash-qualified segment
	// ("example.com/pkg"); the source call site uses only the final path
	// element as the package qualifier.
	if idx := strings.LastIndex(qualifier, "/"); idx >= 0 {
		qualifier = qualifier[idx+1:]
	}
	if qualifier == "" || fn == "" {
		return ""
	}
	return qualifier + "." + fn
}

// RewriteMockCallSites replaces, in file, every call whose callee is a
// selector matching one of subs.QualifiedFunction with that substitution's
// parsed Expression. It returns the number of call sites rewritten.
//
// Matching is purely syntactic: a call `q.Fn(args...)` matches
// "q.Fn" regardless of what `q` resolves to. The whole *ast.CallExpr
// (including its arguments) is replaced, so the mock expression fully
// supplants the original call and its side effects.
//
// Invalid substitution expressions are skipped (reported via the returned
// error) rather than aborting the rewrite of valid ones.
func RewriteMockCallSites(file *ast.File, subs []MockSubstitution) (int, error) {
	if file == nil || len(subs) == 0 {
		return 0, nil
	}

	byKey := make(map[string]string, len(subs))
	var parseErrs []string
	for _, s := range subs {
		if s.QualifiedFunction == "" || strings.TrimSpace(s.Expression) == "" {
			continue
		}
		if _, err := parser.ParseExpr(s.Expression); err != nil {
			parseErrs = append(parseErrs, fmt.Sprintf("%s: %v", s.QualifiedFunction, err))
			continue
		}
		byKey[s.QualifiedFunction] = s.Expression
	}
	if len(byKey) == 0 {
		if len(parseErrs) > 0 {
			return 0, fmt.Errorf("mock substitution: %s", strings.Join(parseErrs, "; "))
		}
		return 0, nil
	}

	count := 0
	astutil.Apply(file, nil, func(c *astutil.Cursor) bool {
		call, ok := c.Node().(*ast.CallExpr)
		if !ok {
			return true
		}
		sel, ok := call.Fun.(*ast.SelectorExpr)
		if !ok {
			return true
		}
		ident, ok := sel.X.(*ast.Ident)
		if !ok {
			return true
		}
		key := ident.Name + "." + sel.Sel.Name
		exprSrc, ok := byKey[key]
		if !ok {
			return true
		}
		// Parse a fresh expression per call site so replaced nodes never
		// share AST identity (which would confuse the printer / later passes).
		repl, err := parser.ParseExpr(exprSrc)
		if err != nil {
			return true
		}
		c.Replace(repl)
		count++
		return true
	})

	if len(parseErrs) > 0 {
		return count, fmt.Errorf("mock substitution: %s", strings.Join(parseErrs, "; "))
	}
	return count, nil
}

// RewriteMockCallSitesInFile parses the Go source at path, applies
// RewriteMockCallSites, and rewrites the file in place when any call site
// changed. It is a no-op (returns 0) when no substitution applies. Comments
// are preserved. Used by the overlay build to substitute mocks into the
// already-instrumented target sources before `go build -overlay` compiles
// them.
func RewriteMockCallSitesInFile(path string, subs []MockSubstitution) (int, error) {
	if len(subs) == 0 {
		return 0, nil
	}
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, path, nil, parser.ParseComments)
	if err != nil {
		return 0, fmt.Errorf("mock substitution: parse %q: %w", path, err)
	}
	count, rewriteErr := RewriteMockCallSites(file, subs)
	if count == 0 {
		return 0, rewriteErr
	}
	var buf strings.Builder
	if err := printer.Fprint(&buf, fset, file); err != nil {
		return 0, fmt.Errorf("mock substitution: print %q: %w", path, err)
	}
	if err := os.WriteFile(path, []byte(buf.String()), 0o644); err != nil {
		return 0, fmt.Errorf("mock substitution: write %q: %w", path, err)
	}
	return count, rewriteErr
}
