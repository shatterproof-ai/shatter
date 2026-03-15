#!/usr/bin/env bash
# Pre-commit: detect staged Rust files and run targeted checks.
set -euo pipefail

staged_files="$(git diff --cached --name-only --diff-filter=ACMR 2>/dev/null || true)"
[[ -z "${staged_files}" ]] && { echo "[skip] No staged files"; exit 0; }

has_core=false; has_cli=false; has_rust_fe=false; has_rust_rt=false; has_ws=false

while IFS= read -r file; do
  case "${file}" in
    shatter-core/*)          has_core=true ;;
    shatter-cli/*)           has_cli=true ;;
    shatter-rust/*)          has_rust_fe=true ;;
    shatter-rust-runtime/*)  has_rust_rt=true ;;
    Cargo.toml|Cargo.lock)   has_ws=true ;;
    examples/*)              has_core=true ;;
  esac
done <<< "${staged_files}"

[[ "${has_ws}" == "true" ]] && { has_core=true; has_cli=true; }
[[ "${has_cli}" == "true" ]] && has_core=true

if [[ "${has_core}" == "false" && "${has_cli}" == "false" && \
      "${has_rust_fe}" == "false" && "${has_rust_rt}" == "false" ]]; then
  echo "[skip] No Rust changes staged"; exit 0
fi

pkg_args=()
[[ "${has_core}" == "true" ]] && pkg_args+=(-p shatter-core)
[[ "${has_cli}" == "true" ]] && pkg_args+=(-p shatter-cli)

if [[ ${#pkg_args[@]} -gt 0 ]]; then
  echo "==> Rust tests (${pkg_args[*]})"; cargo test "${pkg_args[@]}"
  echo "==> Rust clippy (${pkg_args[*]})"; cargo clippy "${pkg_args[@]}" -- -D warnings
fi
[[ "${has_rust_fe}" == "true" ]] && (cd shatter-rust && cargo test && cargo clippy -- -D warnings)
[[ "${has_rust_rt}" == "true" ]] && (cd shatter-rust-runtime && cargo test && cargo clippy -- -D warnings)

echo "[ok] Pre-commit checks passed"
