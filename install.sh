#!/usr/bin/env bash
# Shatter installer — downloads the correct platform binary from GitHub Releases.
# Usage: curl -sSL https://raw.githubusercontent.com/shatterproof-ai/shatter/main/install.sh | bash
#
# Environment variables:
#   INSTALL_DIR  — where to place the binary (default: ~/.local/bin)
#   VERSION      — version to install (default: latest)

set -euo pipefail

REPO="shatterproof-ai/shatter"
BINARY="shatter"
DEFAULT_INSTALL_DIR="$HOME/.local/bin"
INSTALL_DIR="${INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"

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

resolve_version() {
  if [ -n "${VERSION:-}" ]; then
    return
  fi

  info "Fetching" "latest release version..."
  VERSION=$(curl -sSfL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' \
    | sed -E 's/.*"tag_name":\s*"([^"]+)".*/\1/')

  if [ -z "$VERSION" ]; then
    error "Could not determine latest version. Set VERSION explicitly or check https://github.com/${REPO}/releases"
  fi
}

download() {
  local url="https://github.com/${REPO}/releases/download/${VERSION}/${BINARY}-${PLATFORM}.tar.gz"

  info "Downloading" "${BINARY} ${VERSION} for ${PLATFORM}..."
  info "URL" "$url"

  local tmpdir
  tmpdir=$(mktemp -d)
  trap 'rm -rf "$tmpdir"' EXIT

  if ! curl -sSfL "$url" -o "$tmpdir/archive.tar.gz"; then
    error "Download failed. Release ${VERSION} may not have a binary for ${PLATFORM}.
  Check available assets at: https://github.com/${REPO}/releases/tag/${VERSION}"
  fi

  tar -xzf "$tmpdir/archive.tar.gz" -C "$tmpdir"

  if [ ! -f "$tmpdir/${BINARY}" ]; then
    error "Archive did not contain expected binary '${BINARY}'."
  fi

  mkdir -p "$INSTALL_DIR"
  mv "$tmpdir/${BINARY}" "$INSTALL_DIR/${BINARY}"
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
  resolve_version
  download
  verify
  check_path

  echo ""
  info "Done!" "Run 'shatter --help' to get started."
  echo ""
}

main
