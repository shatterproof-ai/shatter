package protocol

import (
	"strings"
	"testing"

	"golang.org/x/tools/go/packages"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
)

// loadMockIdentityPkg loads testdata/mock_identity_project/main.go with full
// type info. The fixture imports two packages both declared `package auth`
// (mockidentity/a/auth and mockidentity/b/auth) under aliases autha/authb, so
// source spelling alone cannot tell them apart — only the resolved import path.
func loadMockIdentityPkg(t *testing.T) *packages.Package {
	t.Helper()
	ldr, cleanup, err := newTransientLoader()
	if err != nil {
		t.Fatalf("newTransientLoader: %v", err)
	}
	t.Cleanup(cleanup)
	file := testFilePath(t, "mock_identity_project/main.go")
	pkg, err := loadPackageForAnalysis(ldr, file)
	if err != nil {
		t.Fatalf("loadPackageForAnalysis: %v", err)
	}
	return pkg
}

// resolvedByFunc indexes resolved substitutions by (localSpelling → enclosing
// function → expression) so a test can assert which call sites each mock owns.
func resolvedByFunc(subs []instrument.MockSubstitution) map[string]map[string]string {
	out := map[string]map[string]string{}
	for _, s := range subs {
		if out[s.QualifiedFunction] == nil {
			out[s.QualifiedFunction] = map[string]string{}
		}
		for fn := range s.AllowedFuncs {
			out[s.QualifiedFunction][fn] = s.Expression
		}
	}
	return out
}

// TestResolveMockSubstitutionScopes_AliasedImportMatches is the str-djcv2
// regression for consequence 2: a BARE config mock ("auth.GetName") matches
// aliased call sites (autha.GetName, authb.GetName) because matching keys on
// resolved package identity, not the source qualifier. Because "auth" resolves
// to two distinct import paths here, both are rewritten and an ambiguity
// warning is emitted.
func TestResolveMockSubstitutionScopes_AliasedImportMatches(t *testing.T) {
	pkg := loadMockIdentityPkg(t)
	subs := instrument.MockSubstitutionsFromConfigs([]instrument.MockConfig{
		{Symbol: "auth.GetName", Expression: `"mocked"`},
	})
	var warnedAmbiguous bool
	resolved := resolveMockSubstitutionScopes(pkg, subs, func(msg string, args ...any) {
		if strings.Contains(msg, "multiple packages") {
			warnedAmbiguous = true
		}
	})

	idx := resolvedByFunc(resolved)
	if got := idx["autha.GetName"]["UseA"]; got != `"mocked"` {
		t.Errorf("aliased call autha.GetName in UseA not resolved (got %q):\n%+v", got, resolved)
	}
	if got := idx["authb.GetName"]["UseB"]; got != `"mocked"` {
		t.Errorf("aliased call authb.GetName in UseB not resolved (got %q):\n%+v", got, resolved)
	}
	if !warnedAmbiguous {
		t.Errorf("expected an ambiguity warning for bare qualifier resolving to two packages")
	}
}

// TestResolveMockSubstitutionScopes_PathQualifiedDisambiguates is the str-djcv2
// regression for consequence 1: a PATH-QUALIFIED config mock
// ("mockidentity/a/auth.GetName") matches only the call site whose qualifier
// resolves to that exact import path (UseA), never the same-base sibling (UseB).
func TestResolveMockSubstitutionScopes_PathQualifiedDisambiguates(t *testing.T) {
	pkg := loadMockIdentityPkg(t)
	subs := instrument.MockSubstitutionsFromConfigs([]instrument.MockConfig{
		{Symbol: "mockidentity/a/auth.GetName", Expression: `"only-a"`},
	})
	var warnedAmbiguous bool
	resolved := resolveMockSubstitutionScopes(pkg, subs, func(msg string, args ...any) {
		if strings.Contains(msg, "multiple packages") {
			warnedAmbiguous = true
		}
	})

	idx := resolvedByFunc(resolved)
	if got := idx["autha.GetName"]["UseA"]; got != `"only-a"` {
		t.Errorf("path-qualified mock did not resolve its own package's call site (got %q):\n%+v", got, resolved)
	}
	// UseB calls mockidentity/b/auth.GetName — the path-qualified a-mock must
	// NOT own it.
	if _, ok := idx["authb.GetName"]["UseB"]; ok {
		t.Errorf("path-qualified a-mock wrongly claimed b's call site:\n%+v", resolved)
	}
	if warnedAmbiguous {
		t.Errorf("path-qualified mock must not trigger the base-name ambiguity warning")
	}
}
