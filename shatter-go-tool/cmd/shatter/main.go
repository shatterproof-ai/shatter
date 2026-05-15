package main

import (
	"archive/tar"
	"compress/gzip"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"
)

const (
	defaultRepo = "shatterproof-ai/shatter"
	binaryName  = "shatter"
)

type release struct {
	TagName    string    `json:"tag_name"`
	Prerelease bool      `json:"prerelease"`
	CreatedAt  time.Time `json:"created_at"`
}

type manifest struct {
	Tag    string  `json:"tag"`
	Assets []asset `json:"assets"`
}

type asset struct {
	Platform string `json:"platform"`
	Name     string `json:"name"`
	URL      string `json:"url"`
	SHA256   string `json:"sha256"`
}

func main() {
	options, forwarded, err := parseArgs(os.Args[1:])
	if err != nil {
		fmt.Fprintf(os.Stderr, "shatter wrapper: %v\n", err)
		os.Exit(2)
	}

	binary, err := ensureBinary(options.repo, options.build)
	if err != nil {
		fmt.Fprintf(os.Stderr, "shatter wrapper: %v\n", err)
		os.Exit(1)
	}

	cmd := exec.Command(binary, forwarded...)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	if err := cmd.Run(); err != nil {
		var exitErr *exec.ExitError
		if errors.As(err, &exitErr) {
			os.Exit(exitErr.ExitCode())
		}
		fmt.Fprintf(os.Stderr, "shatter wrapper: %v\n", err)
		os.Exit(1)
	}
}

type wrapperOptions struct {
	build string
	repo  string
}

func parseArgs(args []string) (wrapperOptions, []string, error) {
	options := wrapperOptions{
		build: os.Getenv("SHATTER_BUILD"),
		repo:  envDefault("SHATTER_REPO", defaultRepo),
	}
	forwarded := make([]string, 0, len(args))

	for index := 0; index < len(args); index++ {
		arg := args[index]
		switch {
		case arg == "--":
			forwarded = append(forwarded, args[index+1:]...)
			return options, forwarded, nil
		case strings.HasPrefix(arg, "--shatter-build="):
			options.build = strings.TrimPrefix(arg, "--shatter-build=")
		case arg == "--shatter-build":
			index++
			if index >= len(args) {
				return options, nil, fmt.Errorf("--shatter-build requires a value")
			}
			options.build = args[index]
		case strings.HasPrefix(arg, "--shatter-repo="):
			options.repo = strings.TrimPrefix(arg, "--shatter-repo=")
		case arg == "--shatter-repo":
			index++
			if index >= len(args) {
				return options, nil, fmt.Errorf("--shatter-repo requires a value")
			}
			options.repo = args[index]
		case arg == "--shatter-wrapper-help":
			printWrapperHelp()
			os.Exit(0)
		default:
			forwarded = append(forwarded, arg)
		}
	}

	return options, forwarded, nil
}

func printWrapperHelp() {
	fmt.Fprintf(os.Stdout, `Usage: shatter [wrapper options] [--] [shatter args]

Wrapper options:
  --shatter-build TAG  Exact continuous build tag to download
  --shatter-repo REPO  GitHub repository that publishes Shatter releases

Environment:
  SHATTER_BUILD        Exact continuous build tag
  SHATTER_REPO         GitHub repository, default %s
  SHATTER_BINARY       Existing shatter binary to exec instead of downloading
`, defaultRepo)
}

func envDefault(name string, fallback string) string {
	if value := os.Getenv(name); value != "" {
		return value
	}
	return fallback
}

func ensureBinary(repo string, build string) (string, error) {
	if override := os.Getenv("SHATTER_BINARY"); override != "" {
		return override, nil
	}
	if build == "" || build == "latest" || build == "continuous" {
		resolved, err := latestContinuousBuild(repo)
		if err != nil {
			return "", err
		}
		build = resolved
	}

	platform, err := currentPlatform()
	if err != nil {
		return "", err
	}

	cacheRoot, err := os.UserCacheDir()
	if err != nil {
		return "", err
	}
	targetDir := filepath.Join(cacheRoot, "shatter", "binaries", build, platform)
	targetBinary := filepath.Join(targetDir, binaryName)
	if isExecutable(targetBinary) {
		return targetBinary, nil
	}

	releaseManifest, err := fetchManifest(repo, build)
	if err != nil {
		return "", err
	}
	selected, err := selectAsset(releaseManifest, platform)
	if err != nil {
		return "", err
	}

	tmpDir, err := os.MkdirTemp("", "shatter-go-tool-*")
	if err != nil {
		return "", err
	}
	defer os.RemoveAll(tmpDir)

	archivePath := filepath.Join(tmpDir, selected.Name)
	if err := downloadFile(selected.URL, archivePath); err != nil {
		return "", err
	}
	if err := verifySHA256(archivePath, selected.SHA256); err != nil {
		return "", err
	}
	if err := os.MkdirAll(targetDir, 0o755); err != nil {
		return "", err
	}
	if err := extractBinary(archivePath, targetBinary); err != nil {
		return "", err
	}
	return targetBinary, nil
}

func latestContinuousBuild(repo string) (string, error) {
	var releases []release
	if err := getJSON(fmt.Sprintf("https://api.github.com/repos/%s/releases", repo), &releases); err != nil {
		return "", err
	}
	var selected release
	for _, candidate := range releases {
		if !candidate.Prerelease || !strings.HasPrefix(candidate.TagName, "continuous-") {
			continue
		}
		if selected.TagName == "" || candidate.CreatedAt.After(selected.CreatedAt) {
			selected = candidate
		}
	}
	if selected.TagName == "" {
		return "", fmt.Errorf("no continuous prerelease found for %s; set SHATTER_BUILD to an exact tag", repo)
	}
	return selected.TagName, nil
}

func currentPlatform() (string, error) {
	var osPart string
	switch runtime.GOOS {
	case "linux":
		osPart = "linux"
	case "darwin":
		osPart = "darwin"
	default:
		return "", fmt.Errorf("unsupported OS %s", runtime.GOOS)
	}

	var archPart string
	switch runtime.GOARCH {
	case "amd64":
		archPart = "x86_64"
	case "arm64":
		archPart = "aarch64"
	default:
		return "", fmt.Errorf("unsupported architecture %s", runtime.GOARCH)
	}
	return osPart + "-" + archPart, nil
}

func fetchManifest(repo string, build string) (manifest, error) {
	var releaseManifest manifest
	url := fmt.Sprintf("https://github.com/%s/releases/download/%s/shatter-release.json", repo, build)
	if err := getJSON(url, &releaseManifest); err != nil {
		return manifest{}, err
	}
	return releaseManifest, nil
}

func selectAsset(releaseManifest manifest, platform string) (asset, error) {
	for _, candidate := range releaseManifest.Assets {
		if candidate.Platform == platform {
			return candidate, nil
		}
	}
	return asset{}, fmt.Errorf("release %s does not advertise %s", releaseManifest.Tag, platform)
}

func getJSON(url string, target any) error {
	req, err := http.NewRequest(http.MethodGet, url, nil)
	if err != nil {
		return err
	}
	req.Header.Set("Accept", "application/vnd.github+json")
	req.Header.Set("User-Agent", "shatter-go-tool")
	if token := os.Getenv("GITHUB_TOKEN"); token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("GET %s returned %s", url, resp.Status)
	}
	return json.NewDecoder(resp.Body).Decode(target)
}

func downloadFile(url string, dest string) error {
	resp, err := http.Get(url)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("GET %s returned %s", url, resp.Status)
	}

	out, err := os.Create(dest)
	if err != nil {
		return err
	}
	defer out.Close()
	_, err = io.Copy(out, resp.Body)
	return err
}

func verifySHA256(path string, expected string) error {
	file, err := os.Open(path)
	if err != nil {
		return err
	}
	defer file.Close()

	hash := sha256.New()
	if _, err := io.Copy(hash, file); err != nil {
		return err
	}
	actual := hex.EncodeToString(hash.Sum(nil))
	if actual != expected {
		return fmt.Errorf("checksum mismatch for %s: expected %s, got %s", filepath.Base(path), expected, actual)
	}
	return nil
}

func extractBinary(archivePath string, targetBinary string) error {
	file, err := os.Open(archivePath)
	if err != nil {
		return err
	}
	defer file.Close()

	gzipReader, err := gzip.NewReader(file)
	if err != nil {
		return err
	}
	defer gzipReader.Close()

	tarReader := tar.NewReader(gzipReader)
	for {
		header, err := tarReader.Next()
		if errors.Is(err, io.EOF) {
			return fmt.Errorf("archive did not contain %s", binaryName)
		}
		if err != nil {
			return err
		}
		if filepath.Base(header.Name) != binaryName || header.Typeflag != tar.TypeReg {
			continue
		}
		out, err := os.OpenFile(targetBinary, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o755)
		if err != nil {
			return err
		}
		if _, err := io.Copy(out, tarReader); err != nil {
			out.Close()
			return err
		}
		return out.Close()
	}
}

func isExecutable(path string) bool {
	info, err := os.Stat(path)
	return err == nil && !info.IsDir() && info.Mode()&0o111 != 0
}
