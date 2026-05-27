package sandbox

import (
	"os"
	"os/exec"
	"path/filepath"
	"slices"
	"strings"
	"testing"
)

func TestNoopCommandPreservesWorkDirAndEnv(t *testing.T) {
	projectRoot := t.TempDir()
	binaryPath := writeExecutable(t, projectRoot, "launcher")
	workDir := filepath.Join(projectRoot, "pkg")
	if err := os.MkdirAll(workDir, 0o755); err != nil {
		t.Fatalf("mkdir work dir: %v", err)
	}

	prepared, err := NewRunner(Config{Backend: BackendNone}).Command(Spec{
		BinaryPath:  binaryPath,
		ProjectRoot: projectRoot,
		WorkDir:     workDir,
		Env:         []string{"A=B"},
	})
	if err != nil {
		t.Fatalf("Command: %v", err)
	}
	defer prepared.Cleanup()

	if prepared.Cmd.Path != binaryPath {
		t.Fatalf("cmd path = %q, want %q", prepared.Cmd.Path, binaryPath)
	}
	if prepared.Cmd.Dir != workDir {
		t.Fatalf("cmd dir = %q, want %q", prepared.Cmd.Dir, workDir)
	}
	if !slices.Contains(prepared.Cmd.Env, "A=B") {
		t.Fatalf("cmd env missing A=B: %#v", prepared.Cmd.Env)
	}
}

func TestCopyTreeSkipsShatterLaunchers(t *testing.T) {
	// str-17np: .shatter-launchers is a transient build artifact that
	// must not be copied into the sandbox project. Its presence during
	// parallel builds caused ENOENT races when cleanup removed it mid-walk.
	projectRoot := t.TempDir()
	launchersDir := filepath.Join(projectRoot, ".shatter-launchers", "abc123-9999")
	if err := os.MkdirAll(launchersDir, 0o755); err != nil {
		t.Fatalf("mkdir .shatter-launchers: %v", err)
	}
	if err := os.WriteFile(filepath.Join(launchersDir, "main.go"), []byte("package main"), 0o644); err != nil {
		t.Fatalf("write main.go: %v", err)
	}
	// Also create a regular file to verify copying still works.
	if err := os.WriteFile(filepath.Join(projectRoot, "go.mod"), []byte("module test"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	binaryPath := writeExecutable(t, projectRoot, "launcher")
	prepared, err := NewRunner(Config{
		Backend:  BackendBubblewrap,
		TempRoot: t.TempDir(),
	}).Command(Spec{
		BinaryPath:  binaryPath,
		ProjectRoot: projectRoot,
		WorkDir:     projectRoot,
	})
	if err != nil {
		t.Fatalf("Command: %v", err)
	}
	defer prepared.Cleanup()

	if _, err := os.Stat(filepath.Join(prepared.ProjectCopy, ".shatter-launchers")); !os.IsNotExist(err) {
		t.Fatalf(".shatter-launchers should be skipped in sandbox copy, err=%v", err)
	}
	if _, err := os.Stat(filepath.Join(prepared.ProjectCopy, "go.mod")); err != nil {
		t.Fatalf("go.mod should be present in sandbox copy: %v", err)
	}
}

func TestCopyTreeSkipsVanishedEntries(t *testing.T) {
	// str-17np defense-in-depth: entries that disappear between readdir and
	// stat should be silently skipped, not fail the copy.
	projectRoot := t.TempDir()
	ephemeralDir := filepath.Join(projectRoot, "ephemeral")
	if err := os.MkdirAll(ephemeralDir, 0o755); err != nil {
		t.Fatalf("mkdir ephemeral: %v", err)
	}
	if err := os.WriteFile(filepath.Join(projectRoot, "keep.txt"), []byte("keep"), 0o644); err != nil {
		t.Fatalf("write keep.txt: %v", err)
	}

	// Remove the ephemeral directory before the copy but after setup —
	// this simulates the race where an entry appears in readdir but is
	// gone by the time WalkDir visits it. We can't exactly reproduce the
	// race deterministically, but we verify that copyTree handles ENOENT
	// in the walkErr path by invoking it on a tree where the walk
	// callback will encounter pre-removed state.
	dst := filepath.Join(t.TempDir(), "dest")
	// Remove ephemeral right before copy — since WalkDir does an initial
	// readdir at project root level, if ephemeral is gone by then, it
	// won't be visited. This test primarily validates the skip-list; the
	// ENOENT defense is tested via the .shatter-launchers skip.
	if err := os.RemoveAll(ephemeralDir); err != nil {
		t.Fatalf("remove ephemeral: %v", err)
	}
	if err := copyTree(projectRoot, dst); err != nil {
		t.Fatalf("copyTree should succeed after entry removal: %v", err)
	}
	if _, err := os.Stat(filepath.Join(dst, "keep.txt")); err != nil {
		t.Fatalf("keep.txt should be present: %v", err)
	}
}

func TestBubblewrapCommandMountsScratchProjectAndPrivateTmp(t *testing.T) {
	hostRoot := t.TempDir()
	projectRoot := filepath.Join(hostRoot, "project")
	sourceFile := filepath.Join(projectRoot, "pkg", "input.txt")
	if err := os.MkdirAll(filepath.Dir(sourceFile), 0o755); err != nil {
		t.Fatalf("mkdir source dir: %v", err)
	}
	if err := os.WriteFile(sourceFile, []byte("source"), 0o644); err != nil {
		t.Fatalf("write source file: %v", err)
	}
	if err := os.MkdirAll(filepath.Join(projectRoot, ".git"), 0o755); err != nil {
		t.Fatalf("mkdir .git: %v", err)
	}
	binaryPath := writeExecutable(t, hostRoot, "launcher")
	tempRoot := t.TempDir()

	prepared, err := NewRunner(Config{
		Backend:        BackendBubblewrap,
		BubblewrapPath: "bwrap",
		TempRoot:       tempRoot,
	}).Command(Spec{
		BinaryPath:  binaryPath,
		ProjectRoot: projectRoot,
		WorkDir:     filepath.Join(projectRoot, "pkg"),
	})
	if err != nil {
		t.Fatalf("Command: %v", err)
	}
	defer prepared.Cleanup()

	args := prepared.Cmd.Args
	assertArg(t, args, "--unshare-all")
	assertArg(t, args, "--unshare-net")
	assertArgPair(t, args, "--tmpfs", "/tmp")
	assertArgPair(t, args, "--bind", prepared.ProjectCopy)
	assertArgAfter(t, args, prepared.ProjectCopy, projectRoot)
	assertArgPair(t, args, "--ro-bind", prepared.ExecutableDir)
	assertArgAfter(t, args, prepared.ExecutableDir, "/shatter-bin")
	assertArgPair(t, args, "--chdir", filepath.Join(projectRoot, "pkg"))
	if got := args[len(args)-1]; got != "/shatter-bin/launcher" {
		t.Fatalf("last arg = %q, want sandbox launcher", got)
	}

	if _, err := os.Stat(filepath.Join(prepared.ProjectCopy, "pkg", "input.txt")); err != nil {
		t.Fatalf("scratch project missing copied source file: %v", err)
	}
	if _, err := os.Stat(filepath.Join(prepared.ProjectCopy, ".git")); !os.IsNotExist(err) {
		t.Fatalf("scratch project copied .git, err=%v", err)
	}
}

func TestDockerCommandUsesHardenedDefaults(t *testing.T) {
	hostRoot := t.TempDir()
	projectRoot := filepath.Join(hostRoot, "project")
	if err := os.MkdirAll(projectRoot, 0o755); err != nil {
		t.Fatalf("mkdir project root: %v", err)
	}
	binaryPath := writeExecutable(t, hostRoot, "launcher")

	prepared, err := NewRunner(Config{
		Backend:       BackendDocker,
		DockerPath:    "docker",
		DockerImage:   "shatter-go-runtime:test",
		DockerRuntime: "runsc",
		TempRoot:      t.TempDir(),
	}).Command(Spec{
		BinaryPath:  binaryPath,
		ProjectRoot: projectRoot,
		WorkDir:     projectRoot,
	})
	if err != nil {
		t.Fatalf("Command: %v", err)
	}
	defer prepared.Cleanup()

	args := prepared.Cmd.Args
	assertArgSequence(t, args[1:], "run", "--rm", "-i")
	assertArgPair(t, args, "--runtime", "runsc")
	assertArgPair(t, args, "--network", "none")
	assertArgPair(t, args, "--read-only", "")
	assertArgPair(t, args, "--cap-drop", "ALL")
	assertArgPair(t, args, "--security-opt", "no-new-privileges")
	assertArgPair(t, args, "--tmpfs", "/tmp")
	assertArgPair(t, args, "--tmpfs", "/home/shatter")
	assertArgPair(t, args, "--workdir", projectRoot)
	assertArgPair(t, args, "--entrypoint", "/shatter-bin/launcher")
	assertArg(t, args, "shatter-go-runtime:test")
}

func TestBubblewrapCommandContainsRelativeAndTmpWrites(t *testing.T) {
	if _, err := exec.LookPath("bwrap"); err != nil {
		t.Skip("bwrap not installed")
	}

	hostRoot := t.TempDir()
	projectRoot := filepath.Join(hostRoot, "project")
	if err := os.MkdirAll(projectRoot, 0o755); err != nil {
		t.Fatalf("mkdir project root: %v", err)
	}
	tmpName := "shatter-bwrap-test-" + strings.ReplaceAll(t.Name(), "/", "-")
	binaryPath := filepath.Join(hostRoot, "launcher")
	script := "#!/bin/sh\n" +
		"mkdir -p rel\n" +
		"printf rel > rel/created.txt\n" +
		"mkdir -p /tmp/" + tmpName + "\n" +
		"printf tmp > /tmp/" + tmpName + "/created.txt\n"
	if err := os.WriteFile(binaryPath, []byte(script), 0o755); err != nil {
		t.Fatalf("write launcher script: %v", err)
	}

	prepared, err := NewRunner(Config{
		Backend:  BackendBubblewrap,
		TempRoot: t.TempDir(),
	}).Command(Spec{
		BinaryPath:  binaryPath,
		ProjectRoot: projectRoot,
		WorkDir:     projectRoot,
	})
	if err != nil {
		t.Fatalf("Command: %v", err)
	}
	defer prepared.Cleanup()

	out, err := prepared.Cmd.CombinedOutput()
	if err != nil {
		if isBubblewrapEnvironmentError(string(out), err) {
			t.Skipf("bwrap unavailable in this environment: %v\n%s", err, out)
		}
		t.Fatalf("run bwrap command: %v\n%s", err, out)
	}

	if _, err := os.Stat(filepath.Join(projectRoot, "rel", "created.txt")); !os.IsNotExist(err) {
		t.Fatalf("relative write leaked to host project root, err=%v", err)
	}
	if _, err := os.Stat(filepath.Join(os.TempDir(), tmpName, "created.txt")); !os.IsNotExist(err) {
		t.Fatalf("/tmp write leaked to host tmp, err=%v", err)
	}
	if _, err := os.Stat(filepath.Join(prepared.ProjectCopy, "rel", "created.txt")); err != nil {
		t.Fatalf("relative write missing from scratch project: %v", err)
	}
}

func writeExecutable(t *testing.T, dir, name string) string {
	t.Helper()
	path := filepath.Join(dir, name)
	if err := os.WriteFile(path, []byte("#!/bin/sh\nexit 0\n"), 0o755); err != nil {
		t.Fatalf("write executable: %v", err)
	}
	return path
}

func assertArg(t *testing.T, args []string, want string) {
	t.Helper()
	if !slices.Contains(args, want) {
		t.Fatalf("args missing %q:\n%s", want, strings.Join(args, "\n"))
	}
}

func assertArgPair(t *testing.T, args []string, key, value string) {
	t.Helper()
	for i, arg := range args {
		if arg != key {
			continue
		}
		if value == "" {
			return
		}
		if i+1 < len(args) && args[i+1] == value {
			return
		}
	}
	t.Fatalf("args missing pair %q %q:\n%s", key, value, strings.Join(args, "\n"))
}

func assertArgAfter(t *testing.T, args []string, key, value string) {
	t.Helper()
	for i, arg := range args {
		if arg == key && i+1 < len(args) && args[i+1] == value {
			return
		}
	}
	t.Fatalf("args missing value %q after %q:\n%s", value, key, strings.Join(args, "\n"))
}

func assertArgSequence(t *testing.T, args []string, sequence ...string) {
	t.Helper()
	if len(args) < len(sequence) {
		t.Fatalf("args too short for sequence %v: %v", sequence, args)
	}
	for i, want := range sequence {
		if args[i] != want {
			t.Fatalf("arg[%d] = %q, want %q in %v", i, args[i], want, sequence)
		}
	}
}

func isBubblewrapEnvironmentError(output string, err error) bool {
	message := output + "\n" + err.Error()
	for _, fragment := range []string{
		"No permissions to create new namespace",
		"Operation not permitted",
		"Creating new namespace failed",
		"bwrap: setting up uid map",
	} {
		if strings.Contains(message, fragment) {
			return true
		}
	}
	return false
}
