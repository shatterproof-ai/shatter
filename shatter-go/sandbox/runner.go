// Package sandbox builds OS-level containment commands for Go harnesses.
package sandbox

import (
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
)

const (
	// EnvironmentBackendKey selects the sandbox backend: none, bwrap, or docker.
	EnvironmentBackendKey = "SHATTER_SANDBOX_BACKEND"
	// EnvironmentDockerImageKey selects the runtime image for the docker backend.
	EnvironmentDockerImageKey = "SHATTER_SANDBOX_DOCKER_IMAGE"
	// EnvironmentDockerRuntimeKey selects an optional docker OCI runtime, e.g. runsc.
	EnvironmentDockerRuntimeKey = "SHATTER_SANDBOX_DOCKER_RUNTIME"

	defaultDockerImage = "debian:bookworm-slim"
	sandboxLauncher    = "/shatter-bin/launcher"
)

// Backend names a supported sandbox runner.
type Backend string

const (
	BackendNone       Backend = "none"
	BackendBubblewrap Backend = "bwrap"
	BackendDocker     Backend = "docker"
)

// Config controls sandbox command construction.
type Config struct {
	Backend        Backend
	BubblewrapPath string
	DockerPath     string
	DockerImage    string
	DockerRuntime  string
	TempRoot       string
}

// Spec describes the subprocess Shatter needs to run inside containment.
type Spec struct {
	BinaryPath  string
	ProjectRoot string
	WorkDir     string
	Env         []string
}

// Runner prepares sandboxed commands.
type Runner struct {
	config Config
}

// PreparedCommand owns a command and its disposable sandbox filesystem.
type PreparedCommand struct {
	Cmd           *exec.Cmd
	SandboxRoot   string
	ProjectCopy   string
	ExecutableDir string

	cleanupOnce sync.Once
	cleanupErr  error
}

// NewRunner returns a Runner using config defaults for unset fields.
func NewRunner(config Config) Runner {
	if config.Backend == "" {
		config.Backend = BackendNone
	}
	if config.BubblewrapPath == "" {
		config.BubblewrapPath = "bwrap"
	}
	if config.DockerPath == "" {
		config.DockerPath = "docker"
	}
	if config.DockerImage == "" {
		config.DockerImage = defaultDockerImage
	}
	return Runner{config: config}
}

// FromEnv returns a Runner configured from environment variables.
func FromEnv() Runner {
	return NewRunner(Config{
		Backend:       Backend(strings.TrimSpace(os.Getenv(EnvironmentBackendKey))),
		DockerImage:   strings.TrimSpace(os.Getenv(EnvironmentDockerImageKey)),
		DockerRuntime: strings.TrimSpace(os.Getenv(EnvironmentDockerRuntimeKey)),
	})
}

// Enabled reports whether this runner applies OS-level containment.
func (r Runner) Enabled() bool {
	return r.config.Backend != "" && r.config.Backend != BackendNone
}

// Command builds the configured command. The caller owns Cleanup after a nil
// error, whether or not Cmd.Start later succeeds.
func (r Runner) Command(spec Spec, args ...string) (*PreparedCommand, error) {
	spec, err := normalizeSpec(spec)
	if err != nil {
		return nil, err
	}

	switch r.config.Backend {
	case "", BackendNone:
		return r.noopCommand(spec, args...), nil
	case BackendBubblewrap:
		return r.bubblewrapCommand(spec, args...)
	case BackendDocker:
		return r.dockerCommand(spec, args...)
	default:
		return nil, fmt.Errorf("sandbox: unsupported backend %q", r.config.Backend)
	}
}

// Cleanup removes the disposable sandbox filesystem. It is safe to call more
// than once.
func (p *PreparedCommand) Cleanup() error {
	if p == nil || p.SandboxRoot == "" {
		return nil
	}
	p.cleanupOnce.Do(func() {
		p.cleanupErr = os.RemoveAll(p.SandboxRoot)
	})
	return p.cleanupErr
}

func (r Runner) noopCommand(spec Spec, args ...string) *PreparedCommand {
	cmd := exec.Command(spec.BinaryPath, args...) //nolint:gosec
	cmd.Dir = spec.WorkDir
	cmd.Env = spec.Env
	return &PreparedCommand{Cmd: cmd}
}

func (r Runner) bubblewrapCommand(spec Spec, launcherArgs ...string) (*PreparedCommand, error) {
	state, err := prepareFilesystem(r.config.TempRoot, spec)
	if err != nil {
		return nil, err
	}

	args := []string{
		"--unshare-all",
		"--unshare-net",
		"--die-with-parent",
		"--new-session",
		"--proc", "/proc",
		"--dev", "/dev",
		"--tmpfs", "/tmp",
		"--dir", "/home",
		"--tmpfs", "/home/shatter",
		"--setenv", "HOME", "/home/shatter",
		"--setenv", "TMPDIR", "/tmp",
		"--setenv", "XDG_CACHE_HOME", "/home/shatter/.cache",
	}
	args = appendExistingReadOnlyBinds(args, "/usr", "/bin", "/lib", "/lib64", "/etc")
	args = appendDirChain(args, spec.ProjectRoot)
	args = append(args,
		"--bind", state.ProjectCopy, spec.ProjectRoot,
		"--ro-bind", state.ExecutableDir, "/shatter-bin",
		"--chdir", sandboxWorkDir(spec),
		sandboxLauncher,
	)
	args = append(args, launcherArgs...)

	cmd := exec.Command(r.config.BubblewrapPath, args...) //nolint:gosec
	cmd.Env = spec.Env
	state.Cmd = cmd
	return state, nil
}

func (r Runner) dockerCommand(spec Spec, launcherArgs ...string) (*PreparedCommand, error) {
	state, err := prepareFilesystem(r.config.TempRoot, spec)
	if err != nil {
		return nil, err
	}

	args := []string{"run", "--rm", "-i"}
	if runtimeName := strings.TrimSpace(r.config.DockerRuntime); runtimeName != "" {
		args = append(args, "--runtime", runtimeName)
	}
	args = append(args,
		"--network", "none",
		"--read-only",
		"--cap-drop", "ALL",
		"--security-opt", "no-new-privileges",
		"--pids-limit", "128",
		"--memory", "512m",
		"--cpus", "1",
		"--tmpfs", "/tmp",
		"--tmpfs", "/home/shatter",
		"--env", "HOME=/home/shatter",
		"--env", "TMPDIR=/tmp",
		"--env", "XDG_CACHE_HOME=/home/shatter/.cache",
	)
	for _, entry := range effectiveEnv(spec.Env) {
		if shouldForwardEnv(entry) {
			args = append(args, "--env", entry)
		}
	}
	args = append(args,
		"--mount", "type=bind,src="+state.ProjectCopy+",dst="+spec.ProjectRoot+",rw",
		"--mount", "type=bind,src="+state.ExecutableDir+",dst=/shatter-bin,ro",
		"--workdir", sandboxWorkDir(spec),
		"--entrypoint", sandboxLauncher,
		r.config.DockerImage,
	)
	args = append(args, launcherArgs...)

	cmd := exec.Command(r.config.DockerPath, args...) //nolint:gosec
	cmd.Env = spec.Env
	state.Cmd = cmd
	return state, nil
}

func prepareFilesystem(tempRoot string, spec Spec) (*PreparedCommand, error) {
	root, err := os.MkdirTemp(tempRoot, "shatter-sandbox-*")
	if err != nil {
		return nil, fmt.Errorf("sandbox: create root: %w", err)
	}
	state := &PreparedCommand{SandboxRoot: root}
	ok := false
	defer func() {
		if !ok {
			_ = state.Cleanup()
		}
	}()

	state.ProjectCopy = filepath.Join(root, "project")
	if err := copyTree(spec.ProjectRoot, state.ProjectCopy); err != nil {
		return nil, err
	}

	state.ExecutableDir = filepath.Join(root, "bin")
	if err := os.MkdirAll(state.ExecutableDir, 0o755); err != nil {
		return nil, fmt.Errorf("sandbox: create executable dir: %w", err)
	}
	if err := copyFile(spec.BinaryPath, filepath.Join(state.ExecutableDir, "launcher")); err != nil {
		return nil, err
	}

	ok = true
	return state, nil
}

func normalizeSpec(spec Spec) (Spec, error) {
	if strings.TrimSpace(spec.BinaryPath) == "" {
		return Spec{}, fmt.Errorf("sandbox: BinaryPath must not be empty")
	}
	binaryPath, err := filepath.Abs(spec.BinaryPath)
	if err != nil {
		return Spec{}, fmt.Errorf("sandbox: normalize binary path: %w", err)
	}
	if strings.TrimSpace(spec.ProjectRoot) == "" {
		spec.ProjectRoot = filepath.Dir(binaryPath)
	}
	projectRoot, err := filepath.Abs(spec.ProjectRoot)
	if err != nil {
		return Spec{}, fmt.Errorf("sandbox: normalize project root: %w", err)
	}
	if strings.TrimSpace(spec.WorkDir) == "" {
		spec.WorkDir = projectRoot
	}
	workDir, err := filepath.Abs(spec.WorkDir)
	if err != nil {
		return Spec{}, fmt.Errorf("sandbox: normalize work dir: %w", err)
	}
	spec.BinaryPath = filepath.Clean(binaryPath)
	spec.ProjectRoot = filepath.Clean(projectRoot)
	spec.WorkDir = filepath.Clean(workDir)
	return spec, nil
}

func sandboxWorkDir(spec Spec) string {
	rel, err := filepath.Rel(spec.ProjectRoot, spec.WorkDir)
	if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return spec.ProjectRoot
	}
	return spec.WorkDir
}

func appendExistingReadOnlyBinds(args []string, paths ...string) []string {
	for _, path := range paths {
		if _, err := os.Stat(path); err == nil {
			args = append(args, "--ro-bind", path, path)
		}
	}
	return args
}

func appendDirChain(args []string, path string) []string {
	clean := filepath.Clean(path)
	var dirs []string
	for dir := filepath.Dir(clean); dir != "." && dir != string(filepath.Separator); dir = filepath.Dir(dir) {
		if dir == "/tmp" || dir == "/home" {
			continue
		}
		dirs = append(dirs, dir)
	}
	for i := len(dirs) - 1; i >= 0; i-- {
		args = append(args, "--dir", dirs[i])
	}
	return args
}

func copyTree(src, dst string) error {
	return filepath.WalkDir(src, func(path string, entry os.DirEntry, walkErr error) error {
		if walkErr != nil {
			return fmt.Errorf("sandbox: walk project: %w", walkErr)
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return fmt.Errorf("sandbox: project relative path: %w", err)
		}
		if rel == "." {
			return os.MkdirAll(dst, 0o755)
		}
		if entry.IsDir() && shouldSkipProjectDir(entry.Name()) {
			return filepath.SkipDir
		}

		target := filepath.Join(dst, rel)
		info, err := entry.Info()
		if err != nil {
			return fmt.Errorf("sandbox: stat project entry %q: %w", path, err)
		}
		mode := info.Mode()
		switch {
		case mode.IsDir():
			return os.MkdirAll(target, mode.Perm())
		case mode.Type()&os.ModeSymlink != 0:
			linkTarget, err := os.Readlink(path)
			if err != nil {
				return fmt.Errorf("sandbox: read symlink %q: %w", path, err)
			}
			if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
				return fmt.Errorf("sandbox: create symlink parent: %w", err)
			}
			return os.Symlink(linkTarget, target)
		case mode.IsRegular():
			return copyFile(path, target)
		default:
			return nil
		}
	})
}

func copyFile(src, dst string) error {
	info, err := os.Stat(src)
	if err != nil {
		return fmt.Errorf("sandbox: stat %q: %w", src, err)
	}
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return fmt.Errorf("sandbox: create file parent: %w", err)
	}
	in, err := os.Open(src)
	if err != nil {
		return fmt.Errorf("sandbox: open %q: %w", src, err)
	}
	defer in.Close()

	out, err := os.OpenFile(dst, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, info.Mode().Perm())
	if err != nil {
		return fmt.Errorf("sandbox: create %q: %w", dst, err)
	}
	if _, err := io.Copy(out, in); err != nil {
		_ = out.Close()
		return fmt.Errorf("sandbox: copy %q to %q: %w", src, dst, err)
	}
	if err := out.Close(); err != nil {
		return fmt.Errorf("sandbox: close %q: %w", dst, err)
	}
	return nil
}

func shouldSkipProjectDir(name string) bool {
	switch name {
	case ".git", ".shatter-cache", "shatter-artifacts", "node_modules", "target":
		return true
	default:
		return false
	}
}

func effectiveEnv(env []string) []string {
	if env != nil {
		return env
	}
	return os.Environ()
}

func shouldForwardEnv(entry string) bool {
	key, _, ok := strings.Cut(entry, "=")
	if !ok || key == "" {
		return false
	}
	switch key {
	case "HOME", "TMPDIR", "XDG_CACHE_HOME":
		return false
	default:
		return true
	}
}
