// Regression coverage for str-qo1.15.
//
// `go build`'s default `-buildvcs=auto` causes failures when invoked from a
// generated launcher directory that lives under a broken or unprobeable
// VCS tree:
//
//	error obtaining VCS status: exit status 128
//	    Use -buildvcs=false to disable VCS stamping.
//
// Launcher binaries are disposable, generated artifacts; VCS stamping has no
// value for them. BuildLauncher must always pass `-buildvcs=false`.
//
// Two layers of coverage:
//
//   - TestBuildLauncherDisablesBuildVCS_FakeGo (white-box, runs in default
//     test-quick): captures the argv passed to `go build` via a fake-go
//     shim and asserts the flag is present.
//
//   - TestBuildLauncherSucceedsWithBrokenAncestorGit (black-box, real
//     toolchain): plants a broken `.git` directory above the launcher's
//     workspace and proves the build still succeeds. This is the
//     end-to-end repro described in the bug report — a real Go module
//     checkout where the toolchain cannot probe VCS state.
package launcher_test

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/launcher"
)

func TestBuildLauncherDisablesBuildVCS_FakeGo(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("fake go script uses POSIX shell")
	}

	fakeBinDir := t.TempDir()
	argvDir := t.TempDir()
	fakeGo := filepath.Join(fakeBinDir, "go")
	// The fake go records its argv to a file, then synthesises an empty
	// executable at the -o target so BuildLauncher's post-build stat passes.
	const fakeGoScript = `#!/bin/sh
set -eu
printf '%s\n' "$@" > "$SHATTER_FAKE_GO_ARGV_DIR/argv-$$"
out=""
while [ "$#" -gt 0 ]; do
	if [ "$1" = "-o" ]; then
		shift
		out="$1"
	fi
	shift || true
done
if [ -n "$out" ]; then
	mkdir -p "$(dirname "$out")"
	printf '#!/bin/sh\nexit 0\n' > "$out"
	chmod +x "$out"
fi
`
	if err := os.WriteFile(fakeGo, []byte(fakeGoScript), 0o755); err != nil {
		t.Fatalf("write fake go: %v", err)
	}
	t.Setenv("PATH", fakeBinDir+string(os.PathListSeparator)+os.Getenv("PATH"))
	t.Setenv("SHATTER_FAKE_GO_ARGV_DIR", argvDir)

	targetModuleDir := t.TempDir()
	if err := os.WriteFile(
		filepath.Join(targetModuleDir, "go.mod"),
		[]byte("module example.com/targets\n\ngo 1.23\n"),
		0o644,
	); err != nil {
		t.Fatalf("write target go.mod: %v", err)
	}

	workDir := t.TempDir()
	_, _, err := launcher.BuildLauncher(launcher.BuildOptions{
		TargetModulePath: "example.com/targets",
		TargetModuleDir:  targetModuleDir,
		TargetImportPath: "example.com/targets",
		DiscoveryHash:    "qo115fakego000",
		GeneratedDir:     filepath.Join(workDir, "generated"),
		BinariesDir:      filepath.Join(workDir, "binaries"),
	})
	if err != nil {
		t.Fatalf("BuildLauncher: %v", err)
	}

	entries, err := os.ReadDir(argvDir)
	if err != nil {
		t.Fatalf("read argv dir: %v", err)
	}
	if len(entries) != 1 {
		t.Fatalf("fake go argv files = %d, want 1", len(entries))
	}
	argvBytes, err := os.ReadFile(filepath.Join(argvDir, entries[0].Name()))
	if err != nil {
		t.Fatalf("read argv file: %v", err)
	}
	argv := string(argvBytes)
	if !strings.Contains(argv, "\n-buildvcs=false\n") &&
		!strings.HasPrefix(argv, "-buildvcs=false\n") {
		t.Errorf("launcher build argv missing -buildvcs=false; got:\n%s", argv)
	}
}

func TestBuildLauncherSucceedsWithBrokenAncestorGit(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skipf("real go toolchain unavailable: %v", err)
	}

	// Wrap everything in a parent that contains a deliberately broken `.git`.
	// `git status` inside this tree fails with exit 128, which is exactly
	// what `go build -buildvcs=auto` cannot tolerate.
	parent := t.TempDir()
	brokenGit := filepath.Join(parent, ".git")
	if err := os.MkdirAll(brokenGit, 0o755); err != nil {
		t.Fatalf("mkdir broken .git: %v", err)
	}
	// A `.git` directory missing HEAD/config/objects causes:
	//   fatal: not a git repository: '.git'
	// from any `git status` invocation walking up from a descendant.
	if err := os.WriteFile(filepath.Join(brokenGit, "not-a-real-git"), []byte("x"), 0o644); err != nil {
		t.Fatalf("seed broken .git: %v", err)
	}

	targetModuleDir := filepath.Join(parent, "module")
	if err := os.MkdirAll(targetModuleDir, 0o755); err != nil {
		t.Fatalf("mkdir target module: %v", err)
	}
	if err := os.WriteFile(
		filepath.Join(targetModuleDir, "go.mod"),
		[]byte("module example.com/qo115\n\ngo 1.23\n"),
		0o644,
	); err != nil {
		t.Fatalf("write target go.mod: %v", err)
	}
	const targetSrc = `package qo115

func Identity(n int) int { return n }
`
	if err := os.WriteFile(filepath.Join(targetModuleDir, "qo115.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write target source: %v", err)
	}

	workDir := filepath.Join(parent, "work")
	if err := os.MkdirAll(workDir, 0o755); err != nil {
		t.Fatalf("mkdir workDir: %v", err)
	}

	_, _, err := launcher.BuildLauncher(launcher.BuildOptions{
		TargetModulePath: "example.com/qo115",
		TargetModuleDir:  targetModuleDir,
		TargetImportPath: "example.com/qo115",
		DiscoveryHash:    "qo115brokengit0",
		GeneratedDir:     filepath.Join(workDir, "generated"),
		BinariesDir:      filepath.Join(workDir, "binaries"),
		// Custom main: the default launcher main imports
		// target.PlanDescriptor / target.ShatterInvoke which the fixture
		// does not provide. We only care here that `go build` itself
		// succeeds against a broken-VCS ancestor — a trivial main suffices.
		MainSource: "package main\n\nfunc main() {}\n",
		GoEnv:      append(os.Environ(), "GOFLAGS="),
	})
	if err != nil {
		// The pre-fix failure mode is:
		//   error obtaining VCS status: exit status 128
		//   Use -buildvcs=false to disable VCS stamping.
		if strings.Contains(err.Error(), "error obtaining VCS status") {
			t.Fatalf("BuildLauncher hit VCS probe — `-buildvcs=false` is missing from launcher build args: %v", err)
		}
		t.Fatalf("BuildLauncher: %v", err)
	}
}

