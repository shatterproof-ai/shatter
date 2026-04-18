package loader

import (
	"testing"

	"golang.org/x/tools/go/packages"
)

func TestLegalAnchorAcceptanceMatrix(t *testing.T) {
	t.Parallel()

	testCases := []struct {
		name        string
		modulePath  string
		packagePath string
		want        string
	}{
		{
			name:        "non-internal subpackage anchors to module root",
			modulePath:  "acme",
			packagePath: "acme/foo",
			want:        "acme",
		},
		{
			name:        "single internal segment anchors to parent",
			modulePath:  "acme",
			packagePath: "acme/foo/internal/bar",
			want:        "acme/foo",
		},
		{
			name:        "internal directly under module anchors to module root",
			modulePath:  "acme",
			packagePath: "acme/internal/foo/bar",
			want:        "acme",
		},
		{
			name:        "deepest internal parent wins",
			modulePath:  "acme",
			packagePath: "acme/foo/internal/bar/internal/baz",
			want:        "acme/foo/internal/bar",
		},
		{
			name:        "module root anchors to itself",
			modulePath:  "acme",
			packagePath: "acme",
			want:        "acme",
		},
	}

	for _, testCase := range testCases {
		testCase := testCase
		t.Run(testCase.name, func(t *testing.T) {
			t.Parallel()

			anchorPath, err := LegalAnchor(newTestPackage(testCase.modulePath, testCase.packagePath))
			if err != nil {
				t.Fatalf("LegalAnchor: %v", err)
			}
			if anchorPath != testCase.want {
				t.Fatalf("LegalAnchor(%q) = %q, want %q", testCase.packagePath, anchorPath, testCase.want)
			}
		})
	}
}

func TestLegalAnchorRejectsCrossModulePackage(t *testing.T) {
	t.Parallel()

	_, err := LegalAnchor(newTestPackage("acme", "other/internal/foo"))
	if err == nil {
		t.Fatal("LegalAnchor should reject packages outside the target module")
	}
}

func TestLegalAnchorRejectsMissingModuleMetadata(t *testing.T) {
	t.Parallel()

	_, err := LegalAnchor(&packages.Package{PkgPath: "acme/foo"})
	if err == nil {
		t.Fatal("LegalAnchor should reject packages without module metadata")
	}
}

func TestLauncherPackagePathUsesLegalAnchor(t *testing.T) {
	t.Parallel()

	launcherPath, err := LauncherPackagePath(
		newTestPackage("acme", "acme/foo/internal/bar"),
		"deadbeef",
		nil,
	)
	if err != nil {
		t.Fatalf("LauncherPackagePath: %v", err)
	}

	want := "acme/foo/shatter_launcher_deadbeef"
	if launcherPath != want {
		t.Fatalf("LauncherPackagePath = %q, want %q", launcherPath, want)
	}
}

func TestLauncherPackagePathAddsDeterministicCollisionSuffixes(t *testing.T) {
	t.Parallel()

	var lookups []string
	packageExists := func(packagePath string) bool {
		lookups = append(lookups, packagePath)
		return packagePath == "acme/shatter_launcher_deadbeef" ||
			packagePath == "acme/shatter_launcher_deadbeef_2"
	}

	launcherPath, err := LauncherPackagePath(
		newTestPackage("acme", "acme/foo"),
		"deadbeef",
		packageExists,
	)
	if err != nil {
		t.Fatalf("LauncherPackagePath: %v", err)
	}

	want := "acme/shatter_launcher_deadbeef_3"
	if launcherPath != want {
		t.Fatalf("LauncherPackagePath = %q, want %q", launcherPath, want)
	}

	wantLookups := []string{
		"acme/shatter_launcher_deadbeef",
		"acme/shatter_launcher_deadbeef_2",
		"acme/shatter_launcher_deadbeef_3",
	}
	if len(lookups) != len(wantLookups) {
		t.Fatalf("collision checks = %d, want %d", len(lookups), len(wantLookups))
	}
	for index, wantLookup := range wantLookups {
		if lookups[index] != wantLookup {
			t.Fatalf("collision check %d = %q, want %q", index, lookups[index], wantLookup)
		}
	}
}

func TestLauncherPackagePathRejectsEmptyTargetIDHash(t *testing.T) {
	t.Parallel()

	_, err := LauncherPackagePath(newTestPackage("acme", "acme/foo"), "", nil)
	if err == nil {
		t.Fatal("LauncherPackagePath should reject an empty target ID hash")
	}
}

func newTestPackage(modulePath string, packagePath string) *packages.Package {
	return &packages.Package{
		PkgPath: packagePath,
		Module: &packages.Module{
			Path: modulePath,
		},
	}
}
