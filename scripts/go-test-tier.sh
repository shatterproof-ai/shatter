#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 full|short"
}

if [[ $# -eq 1 && ( "$1" == "-h" || "$1" == "--help" ) ]]; then
  usage
  exit 0
fi

if [[ $# -ne 1 ]]; then
  usage >&2
  exit 64
fi

case "$1" in
  full)
    rapid_checks=100
    short_arg=()
    ;;
  short)
    rapid_checks=32
    short_arg=(-short)
    ;;
  *) usage >&2; exit 64 ;;
esac

rapid_packages="$({
  # Dollar expressions belong to Go templates, not the shell.
  # shellcheck disable=SC2016
  go list -f '{{ $package := .ImportPath }}{{ range .TestImports }}{{ if eq . "pgregory.net/rapid" }}{{ $package }}{{ "\n" }}{{ end }}{{ end }}{{ range .XTestImports }}{{ if eq . "pgregory.net/rapid" }}{{ $package }}{{ "\n" }}{{ end }}{{ end }}' ./...
} | sort -u)"

plain_packages=""
while IFS= read -r package; do
  [[ -n "$package" ]] || continue
  if ! grep -Fqx "$package" <<<"$rapid_packages"; then
    plain_packages+=" $package"
  fi
done < <(go list ./...)

if [[ -n "$plain_packages" ]]; then
  # Package import paths cannot contain whitespace.
  # shellcheck disable=SC2086
  go test "${short_arg[@]}" -timeout 30m $plain_packages
fi

if [[ -n "$rapid_packages" ]]; then
  # Package import paths cannot contain whitespace.
  # shellcheck disable=SC2086
  go test "${short_arg[@]}" -timeout 30m $rapid_packages -rapid.checks="$rapid_checks"
fi
