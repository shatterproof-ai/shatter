#!/usr/bin/env bash
# Run Rust quality gates for local development or CI.
#
# Modes:
#   (default)          Run workspace tests + clippy + standalone crate tests/clippy.
#   --precommit        Fast: detect staged Rust files, test only affected crates,
#                      skip standalone crates unless changed. Skips #[ignore] tests.
#   --include-ignored  Also run #[ignore]-d tests (slow Rust frontend integration tests).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

RUN_CLIPPY=true
RUN_E2E=false
RUN_DENY=false
RUN_UDEPS=false
USE_NEXTEST=auto
STRICT_OPTIONAL=false
PRECOMMIT=false
INCLUDE_IGNORED=false
LIB_ONLY=false

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/check-rust.sh [options]

Options:
  --precommit       Fast pre-commit: only test crates with staged changes
  --include-ignored Include #[ignore]-d tests (slow Rust frontend integration tests)
  --no-clippy       Skip cargo clippy
  --e2e             Run the e2e_concolic test target
  --deny            Run cargo-deny if installed
  --udeps           Run cargo-udeps if installed
  --nextest         Prefer cargo-nextest for the main Rust test run
  --no-nextest      Force cargo test even if nextest is available
  --strict-optional Fail if an optional tool requested above is missing
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --precommit)
      PRECOMMIT=true
      ;;
    --include-ignored)
      INCLUDE_IGNORED=true
      ;;
    --no-clippy)
      RUN_CLIPPY=false
      ;;
    --e2e)
      RUN_E2E=true
      ;;
    --deny)
      RUN_DENY=true
      ;;
    --udeps)
      RUN_UDEPS=true
      ;;
    --nextest)
      USE_NEXTEST=true
      ;;
    --no-nextest)
      USE_NEXTEST=false
      ;;
    --strict-optional)
      STRICT_OPTIONAL=true
      ;;
    --precommit)
      # Called from the pre-commit hook; run only lib tests (skip E2E integration
      # tests that require built frontends, which may not be available in worktrees).
      LIB_ONLY=true
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1"
      ;;
  esac
  shift
done

require_cmd cargo "install Rust toolchain"

# Resolve nextest availability: auto-detect unless explicitly forced
HAS_NEXTEST=false
if has_cmd cargo-nextest; then
  HAS_NEXTEST=true
fi

if [[ "${USE_NEXTEST}" == "auto" ]]; then
  USE_NEXTEST="${HAS_NEXTEST}"
fi

# ---------------------------------------------------------------------------
# Helper: run cargo test with the right runner and optional flags
# ---------------------------------------------------------------------------
run_cargo_test() {
  local label="$1"
  shift
  # Remaining args are passed to cargo test / nextest (e.g. -p crate)
  if [[ "${USE_NEXTEST}" == "true" && "${HAS_NEXTEST}" == "true" ]]; then
    if [[ "${INCLUDE_IGNORED}" == "true" ]]; then
      run_cmd "${label}" cargo nextest run --run-ignored all "$@"
    else
      run_cmd "${label}" cargo nextest run "$@"
    fi
  else
    if [[ "${INCLUDE_IGNORED}" == "true" ]]; then
      run_cmd "${label}" cargo test "$@" -- --include-ignored
    else
      run_cmd "${label}" cargo test "$@"
    fi
  fi
}

run_cargo_test_in_dir() {
  local dir="$1"
  local label="$2"
  shift 2
  if [[ "${USE_NEXTEST}" == "true" && "${HAS_NEXTEST}" == "true" ]]; then
    run_in_dir "${dir}" "${label}" cargo nextest run "$@"
  else
    run_in_dir "${dir}" "${label}" cargo test "$@"
  fi
}

# ---------------------------------------------------------------------------
# Precommit mode: detect staged Rust files and test only affected crates
# ---------------------------------------------------------------------------
if [[ "${PRECOMMIT}" == "true" ]]; then
  staged_files="$(git diff --cached --name-only --diff-filter=ACMR 2>/dev/null || true)"

  if [[ -z "${staged_files}" ]]; then
    info "No staged files — skipping Rust checks"
    exit 0
  fi

  # Classify staged files into crate buckets
  has_core=false
  has_cli=false
  has_shatter_rust=false
  has_shatter_rust_runtime=false
  has_workspace_root=false

  while IFS= read -r file; do
    case "${file}" in
      shatter-core/*)          has_core=true ;;
      shatter-cli/*)           has_cli=true ;;
      shatter-rust/*)          has_shatter_rust=true ;;
      shatter-rust-runtime/*)  has_shatter_rust_runtime=true ;;
      Cargo.toml|Cargo.lock)   has_workspace_root=true ;;
      examples/*)              has_core=true ;;  # integration tests reference examples
    esac
  done <<< "${staged_files}"

  # Workspace root changes → test all workspace crates
  if [[ "${has_workspace_root}" == "true" ]]; then
    has_core=true
    has_cli=true
  fi

  # shatter-cli depends on shatter-core — always test core if CLI changed
  if [[ "${has_cli}" == "true" ]]; then
    has_core=true
  fi

  # No Rust changes at all
  if [[ "${has_core}" == "false" && "${has_cli}" == "false" && \
        "${has_shatter_rust}" == "false" && "${has_shatter_rust_runtime}" == "false" ]]; then
    info "No Rust changes staged — skipping"
    exit 0
  fi

  # Build package filter for workspace crates
  pkg_args=()
  if [[ "${has_core}" == "true" ]]; then
    pkg_args+=(-p shatter-core)
  fi
  if [[ "${has_cli}" == "true" ]]; then
    pkg_args+=(-p shatter-cli)
  fi

  if [[ ${#pkg_args[@]} -gt 0 ]]; then
    run_cargo_test "Rust tests (staged crates)" "${pkg_args[@]}"

    if [[ "${RUN_CLIPPY}" == "true" ]]; then
      run_cmd "Rust lint (staged crates)" cargo clippy "${pkg_args[@]}" -- -D warnings
    fi
  fi

  # Standalone crates: only if they have staged changes
  if [[ "${has_shatter_rust}" == "true" ]]; then
    run_cargo_test_in_dir shatter-rust "shatter-rust tests"
    if [[ "${RUN_CLIPPY}" == "true" ]]; then
      run_in_dir shatter-rust "shatter-rust lint" cargo clippy -- -D warnings
    fi
  fi

  if [[ "${has_shatter_rust_runtime}" == "true" ]]; then
    run_cargo_test_in_dir shatter-rust-runtime "shatter-rust-runtime tests"
    if [[ "${RUN_CLIPPY}" == "true" ]]; then
      run_in_dir shatter-rust-runtime "shatter-rust-runtime lint" cargo clippy -- -D warnings
    fi
  fi

  info "Rust pre-commit checks complete"
  exit 0
fi

# When --lib-only, run only lib tests (skip integration tests that need built frontends).
LIB_FLAG=""
if [[ "${LIB_ONLY}" == "true" ]]; then
  LIB_FLAG="--lib"
fi

if [[ "${USE_NEXTEST}" == "true" ]]; then
  if [[ "${HAS_NEXTEST}" == "true" ]]; then
    info "Using cargo-nextest for parallel test execution"
    run_cmd "Rust tests (cargo nextest)" cargo nextest run ${LIB_FLAG}
  elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "cargo-nextest requested but not installed"
  else
    warn "cargo-nextest not installed; falling back to cargo test"
    run_cmd "Rust tests (cargo test)" cargo test ${LIB_FLAG}
  fi
else
  info "Using cargo test (install cargo-nextest for faster runs)"
  run_cmd "Rust tests (cargo test)" cargo test ${LIB_FLAG}
fi

# ---------------------------------------------------------------------------
# Full mode (default)
# ---------------------------------------------------------------------------

run_cargo_test "Rust tests"

if [[ "${RUN_CLIPPY}" == "true" ]]; then
  run_cmd "Rust lint (cargo clippy)" cargo clippy -- -D warnings
fi

# Standalone Rust frontend crates (excluded from workspace)
run_cargo_test_in_dir shatter-rust "shatter-rust tests"
run_cargo_test_in_dir shatter-rust-runtime "shatter-rust-runtime tests"

if [[ "${RUN_CLIPPY}" == "true" ]]; then
  run_in_dir shatter-rust "shatter-rust lint (cargo clippy)" cargo clippy -- -D warnings
  run_in_dir shatter-rust-runtime "shatter-rust-runtime lint (cargo clippy)" cargo clippy -- -D warnings
fi

if [[ "${RUN_E2E}" == "true" ]]; then
  run_cmd "Rust E2E tests" cargo test --test e2e_concolic
fi

if [[ "${RUN_DENY}" == "true" ]]; then
  if has_cmd cargo-deny; then
    run_cmd "Rust dependency policy (cargo deny)" cargo deny check
  elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "cargo-deny requested but not installed"
  else
    skip "Rust dependency policy (missing cargo-deny)"
  fi
fi

if [[ "${RUN_UDEPS}" == "true" ]]; then
  if has_cmd cargo-udeps; then
    run_cmd "Rust unused dependency scan (cargo udeps)" cargo udeps --all-targets
  elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "cargo-udeps requested but not installed"
  else
    skip "Rust unused dependency scan (missing cargo-udeps)"
  fi
fi

info "Rust checks complete"
