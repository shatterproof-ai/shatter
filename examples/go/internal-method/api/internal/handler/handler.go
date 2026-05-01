// Package handler is a deeply-nested internal package used by the str-jeen.32
// regression fixture. Its import path
// `example.com/spike/api/internal/handler` places `internal/` two segments
// below the module root, which is the shape that exercises str-b7zh's
// launcher-module-name anchor fix in
// `shatter-go/launcher/launcher.go::computeLauncherModuleName`.
//
// Without that fix, the synthesised launcher module path would be
// `example.com/spike/shatter_launcher_<hash>` — outside the
// `example.com/spike/api/` subtree that Go's internal-visibility rule
// requires for an importer of `example.com/spike/api/internal/handler`.
// With the fix the launcher module path is anchored at
// `example.com/spike/api/shatter_launcher_<hash>`, satisfying the rule.
//
// Cross-ref: str-b7zh, str-jeen.32. See
// `TestLauncherBuildsForInternalFixture` in
// `shatter-go/launcher/launcher_e2e_test.go`.
package handler

// Classify is a free function with a single branch on its int input. It is
// the launcher target for the str-jeen.32 integration test: pure int -> int
// avoids the launcher's receiver-plan path so the test exercises only the
// module-anchor / build-success contract under regression.
func Classify(x int) int {
	if x > 0 {
		return 1
	}
	return -1
}
