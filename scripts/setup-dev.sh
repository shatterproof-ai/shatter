#!/usr/bin/env bash
# Development environment setup for Shatter.
#
# Installs Rust, Go, and Node dependencies needed to build and test
# all components (shatter-core, shatter-cli, shatter-ts, shatter-go,
# shatter-rust).
#
# Safe to re-run — each step checks whether it's already satisfied.
#
# Usage:
#   ./scripts/setup-dev.sh          # install everything
#   ./scripts/setup-dev.sh --check  # dry-run: report what's missing

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# --- Configuration -----------------------------------------------------------
RUST_EDITION="stable"
GO_APT_PACKAGE="golang-1.23"  # available in noble-updates

# --- Helpers ------------------------------------------------------------------
info()  { printf '  \033[1;34m✓\033[0m %s\n' "$1"; }
warn()  { printf '  \033[1;33m!\033[0m %s\n' "$1"; }
miss()  { printf '  \033[1;31m✗\033[0m %s\n' "$1"; }
step()  { printf '\n\033[1m▸ %s\033[0m\n' "$1"; }

CHECK_ONLY=false
if [[ "${1:-}" == "--check" ]]; then
    CHECK_ONLY=true
fi

MISSING=0

need() {
    # need <label> <condition-command>
    if eval "$2" &>/dev/null; then
        info "$1"
    else
        miss "$1 — not found"
        MISSING=$((MISSING + 1))
    fi
}

# --- Source existing environment (nvm, cargo) ---------------------------------
export NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
# shellcheck disable=SC1091
[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
# shellcheck disable=SC1091
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

# --- Check / Install: Rust ---------------------------------------------------
step "Rust toolchain"

if command -v rustc &>/dev/null; then
    info "rustc $(rustc --version | awk '{print $2}')"
    info "cargo $(cargo --version | awk '{print $2}')"
else
    miss "Rust not found"
    if $CHECK_ONLY; then
        MISSING=$((MISSING + 1))
    else
        warn "Installing Rust (${RUST_EDITION})..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
            | sh -s -- -y --default-toolchain "$RUST_EDITION" 2>&1 | tail -3
        # shellcheck disable=SC1091
        . "$HOME/.cargo/env"
        info "Installed rustc $(rustc --version | awk '{print $2}')"
    fi
fi

# --- Check / Install: Go -----------------------------------------------------
step "Go toolchain"

if command -v go &>/dev/null; then
    info "go $(go version | grep -oP 'go\K[0-9]+\.[0-9]+(\.[0-9]+)?')"
else
    miss "Go not found"
    if $CHECK_ONLY; then
        MISSING=$((MISSING + 1))
    else
        warn "Installing ${GO_APT_PACKAGE} via apt..."
        sudo apt-get update -qq
        sudo apt-get install -y -qq "$GO_APT_PACKAGE" golang-go 2>&1 | tail -3
        # apt installs the binary at /usr/lib/go-X.Y/bin/go and symlinks
        # to /usr/bin/go via the golang-go dependency, so no PATH changes needed.
        info "Installed go $(go version | grep -oP 'go\K[0-9]+\.[0-9]+(\.[0-9]+)?')"
    fi
fi

# --- Check / Install: Node.js ------------------------------------------------
step "Node.js"

if command -v node &>/dev/null; then
    info "node $(node --version)"
    info "npm $(npm --version)"
else
    miss "Node.js not found"
    if $CHECK_ONLY; then
        MISSING=$((MISSING + 1))
    else
        echo "  Install Node.js 22+ via nvm or your package manager, then re-run."
        exit 1
    fi
fi

# --- Check: System libraries -------------------------------------------------
step "System libraries"

need "gcc" "command -v gcc"
need "libclang (libclang1-18 or libclang-dev)" "dpkg -l libclang1-18 || dpkg -l libclang-dev"

# --- Configure bindgen (GCC headers for z3-sys) ------------------------------
step "Bindgen configuration"

if [ -f "$REPO_ROOT/.cargo/config.toml" ]; then
    info ".cargo/config.toml already exists"
elif dpkg -l libclang-dev &>/dev/null 2>&1; then
    info "libclang-dev installed — no bindgen workaround needed"
else
    if $CHECK_ONLY; then
        miss ".cargo/config.toml missing (run scripts/configure-bindgen.sh)"
        MISSING=$((MISSING + 1))
    else
        warn "Running configure-bindgen.sh..."
        bash "$REPO_ROOT/scripts/configure-bindgen.sh"
    fi
fi

# --- Install Node dependencies -----------------------------------------------
step "Node dependencies (shatter-ts)"

if [ -d "$REPO_ROOT/shatter-ts/node_modules" ]; then
    info "node_modules present"
else
    if $CHECK_ONLY; then
        miss "node_modules missing (run npm install)"
        MISSING=$((MISSING + 1))
    else
        warn "Running npm install..."
        (cd "$REPO_ROOT/shatter-ts" && npm install --no-audit --no-fund 2>&1 | tail -3)
        info "npm install complete"
    fi
fi

# --- Build check (only in full mode) -----------------------------------------
run_build() {
    # run_build <label> <dir> <command...>
    # Shows full output on failure.
    local label="$1" dir="$2"
    shift 2
    warn "Building ${label}..."
    local output
    if output="$(cd "$dir" && "$@" 2>&1)"; then
        info "${label} build succeeded"
    else
        echo "$output"
        miss "${label} build FAILED"
        return 1
    fi
}

if ! $CHECK_ONLY; then
    step "Verification builds"

    run_build "Rust workspace"  "$REPO_ROOT"              cargo build
    run_build "shatter-rust"    "$REPO_ROOT/shatter-rust"  cargo build
    run_build "shatter-ts"      "$REPO_ROOT/shatter-ts"    npx tsc
    run_build "shatter-go"      "$REPO_ROOT/shatter-go"    go build ./...
fi

# --- Beads issue tracker -----------------------------------------------------
step "Beads issue tracker"

if command -v bd &>/dev/null; then
    info "bd $(bd version 2>&1 | head -1 | awk '{print $3}')"
else
    miss "bd (beads) not found"
    if $CHECK_ONLY; then
        MISSING=$((MISSING + 1))
    else
        if command -v go &>/dev/null; then
            warn "Installing beads via go install..."
            go install github.com/beads-dev/beads/cmd/bd@latest 2>&1 | tail -3 || true
            if command -v bd &>/dev/null; then
                info "Installed bd $(bd version 2>&1 | head -1 | awk '{print $3}')"
            else
                warn "go install failed — install bd manually. See https://github.com/steveyegge/beads"
            fi
        else
            warn "Install bd manually. See https://github.com/steveyegge/beads"
        fi
    fi
fi

# Restore beads database from backup if Dolt database is missing (fresh clone)
if [ -d "$REPO_ROOT/.beads" ] && [ ! -d "$REPO_ROOT/.beads/dolt" ]; then
    if command -v bd &>/dev/null; then
        if $CHECK_ONLY; then
            miss "Beads database not initialized (run setup to restore from backup)"
            MISSING=$((MISSING + 1))
        else
            warn "Beads database missing (fresh clone) — initializing and restoring..."
            (cd "$REPO_ROOT" && bd init --prefix=str 2>&1 | tail -3) || true
            if [ -f "$REPO_ROOT/.beads/backup/issues.jsonl" ]; then
                warn "Restoring from JSONL backup..."
                (cd "$REPO_ROOT" && bd backup restore 2>&1 | tail -3)
                info "Beads database restored from backup"
            elif [ -f "$REPO_ROOT/.beads/issues.jsonl" ]; then
                warn "Restoring from issues.jsonl..."
                (cd "$REPO_ROOT" && bd import "$REPO_ROOT/.beads/issues.jsonl" 2>&1 | tail -3) || true
                info "Beads database restored from issues.jsonl"
            else
                warn "No backup found — beads database will be empty"
            fi
        fi
    fi
elif [ -d "$REPO_ROOT/.beads/dolt" ] && command -v bd &>/dev/null; then
    info "Beads database present"
fi

# Install beads git hooks if missing
if command -v bd &>/dev/null && [ -d "$REPO_ROOT/.beads" ]; then
    if [ ! -f "$REPO_ROOT/.git/hooks/pre-push" ] || ! grep -q "BEADS" "$REPO_ROOT/.git/hooks/pre-push" 2>/dev/null; then
        if $CHECK_ONLY; then
            miss "Beads git hooks not installed (run bd hooks install)"
            MISSING=$((MISSING + 1))
        else
            warn "Installing beads git hooks..."
            (cd "$REPO_ROOT" && bd hooks install 2>&1 | tail -3)
            info "Beads git hooks installed"
        fi
    else
        info "Beads git hooks installed"
    fi
fi

# Install Shatter quality hooks (appends to existing hooks, idempotent)
if [ -f "$REPO_ROOT/scripts/setup-hooks.sh" ]; then
    if $CHECK_ONLY; then
        "$REPO_ROOT/scripts/setup-hooks.sh" --check || MISSING=$((MISSING + 1))
    else
        "$REPO_ROOT/scripts/setup-hooks.sh"
    fi
fi

# --- Summary ------------------------------------------------------------------
echo ""
if $CHECK_ONLY; then
    if [ "$MISSING" -eq 0 ]; then
        info "All dependencies satisfied. Ready to build."
    else
        miss "${MISSING} missing dependency/dependencies. Run without --check to install."
        exit 1
    fi
else
    info "Development environment ready."
    echo ""
    echo "  Quick test:      cargo test"
    echo "  Full test:       cargo test && (cd shatter-ts && npm test) && (cd shatter-go && go test ./...)"
    echo "  Clippy:          cargo clippy -- -D warnings"
    echo "  Issue tracker:   bd ready"
fi
echo ""
