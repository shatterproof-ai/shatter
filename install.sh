#!/usr/bin/env bash
# Shatter installer — downloads the correct platform archive from GitHub Releases.
# Usage: curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | bash
#
# Environment variables:
#   INSTALL_DIR — where to place the binary (default: ~/.local/bin)
#   CHANNEL     — release channel to install: latest, continuous, or nightly (default: latest)
#   BUILD       — exact continuous build tag to install, e.g. continuous-20260512-1735-abc123def456
#   VERSION     — deprecated alias for BUILD

set -euo pipefail

REPO="shatterproof-ai/shatter"
BINARY="shatter"
DEFAULT_INSTALL_DIR="$HOME/.local/bin"
INSTALL_DIR="${INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"
CHANNEL="${CHANNEL:-latest}"

info()  { printf '  \033[1;34m%s\033[0m %s\n' "$1" "$2"; }
error() { printf '  \033[1;31merror:\033[0m %s\n' "$1" >&2; exit 1; }

detect_platform() {
  local os arch

  case "$(uname -s)" in
    Linux*)  os="linux" ;;
    Darwin*) os="darwin" ;;
    *)       error "Unsupported OS: $(uname -s). Only Linux and macOS are supported." ;;
  esac

  case "$(uname -m)" in
    x86_64|amd64)  arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *)             error "Unsupported architecture: $(uname -m). Only x86_64 and aarch64 are supported." ;;
  esac

  PLATFORM="${os}-${arch}"
}

resolve_build() {
  if [ -n "${BUILD:-}" ]; then
    return
  fi

  if [ -n "${VERSION:-}" ]; then
    info "Warning" "VERSION is deprecated; use BUILD for exact continuous build tags."
    BUILD="$VERSION"
    return
  fi

  case "$CHANNEL" in
    latest|continuous|nightly) ;;
    *) error "Unsupported CHANNEL '${CHANNEL}'. Use latest, continuous, nightly, or set BUILD explicitly." ;;
  esac

  if ! command -v python3 >/dev/null 2>&1; then
    error "Resolving the latest continuous build requires python3. Install python3 or set BUILD explicitly."
  fi

  info "Fetching" "latest continuous build tag..."
  BUILD=$(curl -sSfL "https://api.github.com/repos/${REPO}/releases?per_page=100" \
    | python3 -c '
import json
import sys

for release in json.load(sys.stdin):
    tag = release.get("tag_name", "")
    if release.get("prerelease") and tag.startswith("continuous-"):
        print(tag)
        raise SystemExit(0)
raise SystemExit(1)
' || true)

  if [ -z "$BUILD" ]; then
    error "Could not determine latest build. Set BUILD explicitly or check https://github.com/${REPO}/releases"
  fi
}

download_file() {
  local url="$1"
  local dest="$2"
  if ! curl -sSfL "$url" -o "$dest"; then
    error "Download failed: $url"
  fi
}

sha256_file() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  else
    error "No SHA-256 tool found. Install sha256sum or shasum."
  fi
}

archive_name_for_platform() {
  case "$PLATFORM" in
    linux-x86_64) echo "shatter-linux-x86_64.tar.gz" ;;
    linux-aarch64) echo "shatter-linux-aarch64.tar.gz" ;;
    darwin-x86_64) echo "shatter-macos-x86_64.tar.gz" ;;
    darwin-aarch64) echo "shatter-macos-aarch64.tar.gz" ;;
    *) return 1 ;;
  esac
}

manifest_field() {
  local manifest="$1"
  local platform="$2"
  local field="$3"

  if ! command -v python3 >/dev/null 2>&1; then
    return 1
  fi

  python3 - "$manifest" "$platform" "$field" <<'PY'
import json
import sys

manifest_path, platform, field = sys.argv[1:4]
with open(manifest_path, "r", encoding="utf-8") as handle:
    manifest = json.load(handle)

for asset in manifest.get("assets", []):
    if asset.get("platform") == platform:
        value = asset.get(field)
        if value:
            print(value)
            sys.exit(0)

sys.exit(1)
PY
}

download() {
  local tmpdir
  tmpdir=$(mktemp -d)
  trap 'rm -rf "$tmpdir"' EXIT

  local release_url="https://github.com/${REPO}/releases/download/${BUILD}"
  local manifest_url="${release_url}/shatter-release.json"
  local manifest="$tmpdir/shatter-release.json"
  local asset_name
  local asset_url
  local expected_sha

  info "Fetching" "release manifest for ${BUILD}..."
  download_file "$manifest_url" "$manifest"

  if command -v python3 >/dev/null 2>&1; then
    asset_name=$(manifest_field "$manifest" "$PLATFORM" "name" || true)
    asset_url=$(manifest_field "$manifest" "$PLATFORM" "url" || true)
    expected_sha=$(manifest_field "$manifest" "$PLATFORM" "sha256" || true)
  else
    info "Note" "python3 not found; falling back to conventional asset names and SHA256SUMS."
    asset_name=$(archive_name_for_platform || true)
    asset_url="${release_url}/${asset_name}"
    download_file "${release_url}/SHA256SUMS" "$tmpdir/SHA256SUMS"
    expected_sha=$(awk -v name="$asset_name" '$2 == name { print $1 }' "$tmpdir/SHA256SUMS")
  fi

  if [ -z "$asset_name" ] || [ -z "$asset_url" ] || [ -z "$expected_sha" ]; then
    error "Release ${BUILD} does not advertise a ${PLATFORM} asset in shatter-release.json.
  Check available assets at: https://github.com/${REPO}/releases/tag/${BUILD}"
  fi

  info "Downloading" "${BINARY} ${BUILD} for ${PLATFORM}..."
  info "URL" "$asset_url"
  download_file "$asset_url" "$tmpdir/$asset_name"

  local actual_sha
  actual_sha=$(sha256_file "$tmpdir/$asset_name")
  if [ "$actual_sha" != "$expected_sha" ]; then
    error "Checksum mismatch for ${asset_name}.
  expected: ${expected_sha}
  actual:   ${actual_sha}"
  fi

  tar -xzf "$tmpdir/$asset_name" -C "$tmpdir"

  if [ ! -f "$tmpdir/${BINARY}" ]; then
    error "Archive did not contain expected binary '${BINARY}'."
  fi

  mkdir -p "$INSTALL_DIR"
  cp "$tmpdir/${BINARY}" "$INSTALL_DIR/${BINARY}"
  chmod +x "$INSTALL_DIR/${BINARY}"
}

verify() {
  if ! "$INSTALL_DIR/${BINARY}" --version >/dev/null 2>&1; then
    error "Installed binary failed to run. Try running: ${INSTALL_DIR}/${BINARY} --version"
  fi
  local ver
  ver=$("$INSTALL_DIR/${BINARY}" --version)
  info "Installed" "$ver"
}

check_path() {
  case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
      echo ""
      info "Note" "$INSTALL_DIR is not in your PATH."
      echo "  Add it by appending this to your shell profile:"
      echo ""
      echo "    export PATH=\"$INSTALL_DIR:\$PATH\""
      echo ""
      ;;
  esac
}

main() {
  echo ""
  info "Shatter" "installer"
  echo ""

  detect_platform
  resolve_build
  download
  verify
  check_path

  echo ""
  info "Done!" "Run 'shatter --help' to get started."
  echo ""
}

if [ "${SHATTER_INSTALLER_NO_MAIN:-}" != "1" ]; then
  main
fi
