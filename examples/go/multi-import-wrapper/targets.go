// Package targets is the str-jeen.33 multi-import wrapper fixture.
//
// It exercises wrapper-generation against a target whose parameter list pulls
// in ten distinct packages — five stdlib (context, log/slog, os, io, go/ast)
// and five non-stdlib local stubs that mimic common third-party / application
// packages (pgx, gqlerror, model, search, config). Without explicit import
// emission in shatter-go/wrapper.GenerateWrapper, the generated wrapper file
// uses qualified type names like context.Context, *pgx.Conn, etc. while only
// importing encoding/json + fmt — which fails to compile.
//
// The integration test TestLauncherBuildsForMultiImportWrapper in
// shatter-go/launcher/launcher_e2e_test.go drives BuildLauncher against this
// fixture and asserts the build succeeds.
package targets

import (
	"context"
	"go/ast"
	"io"
	"log/slog"
	"os"

	"example.com/multiimport/config"
	"example.com/multiimport/gqlerror"
	"example.com/multiimport/model"
	"example.com/multiimport/pgx"
	"example.com/multiimport/search"
)

// Handle is a free function whose parameter list deliberately spans the ten
// imports listed in str-jeen.33 (context, log/slog, model, search, os, pgx,
// io, go/ast, gqlerror, config). The body is intentionally trivial; only the
// signature is load-bearing for the wrapper-import test.
//
// nolint:revive — long parameter list is the entire point of this fixture.
func Handle(
	ctx context.Context,
	logger *slog.Logger,
	user model.User,
	query search.Query,
	file *os.File,
	conn *pgx.Conn,
	reader io.Reader,
	ident *ast.Ident,
	gqlErr *gqlerror.Error,
	cfg config.Config,
) int {
	// Touch every parameter so unused-arg lints don't fire on real builds of
	// the fixture (the wrapper test compiles the package via go build).
	_ = ctx
	_ = logger
	_ = user
	_ = query
	_ = file
	_ = conn
	_ = reader
	_ = ident
	_ = gqlErr
	_ = cfg
	return 0
}
