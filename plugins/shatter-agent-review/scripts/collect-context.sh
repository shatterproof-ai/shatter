#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: collect-context.sh [--output PATH] [--run-dir PATH] [--target PATH]... [--artifact PATH]...

Collect relevant system and project context for a Shatter issue report.
The script prints markdown to stdout unless --output is provided.
EOF
}

OUTPUT=""
RUN_DIR=""
declare -a TARGETS=()
declare -a ARTIFACTS=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --output)
            OUTPUT="$2"
            shift 2
            ;;
        --run-dir)
            RUN_DIR="$2"
            shift 2
            ;;
        --target)
            TARGETS+=("$2")
            shift 2
            ;;
        --artifact)
            ARTIFACTS+=("$2")
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if [[ -n "$OUTPUT" ]]; then
    exec >"$OUTPUT"
fi

first_line() {
    "$@" 2>/dev/null | head -n 1 || true
}

file_size() {
    local path="$1"
    if stat -c '%s bytes' "$path" >/dev/null 2>&1; then
        stat -c '%s bytes' "$path"
    elif stat -f '%z bytes' "$path" >/dev/null 2>&1; then
        stat -f '%z bytes' "$path"
    else
        printf 'size unknown'
    fi
}

print_file_item() {
    local path="$1"
    if [[ -e "$path" ]]; then
        printf -- '- `%s` (%s)\n' "$path" "$(file_size "$path")"
    else
        printf -- '- `%s` (missing)\n' "$path"
    fi
}

print_config_file() {
    local path="$1"
    if [[ -e "$path" ]]; then
        printf -- '- `%s`\n' "$path"
        return 0
    fi
    return 1
}

ROOT_DIR="$(pwd)"
if git_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
    ROOT_DIR="$git_root"
fi

echo "## Captured Context"
echo
echo "### System"
printf -- '- Time (local): `%s`\n' "$(date '+%Y-%m-%d %H:%M:%S %Z' 2>/dev/null || printf 'unknown')"
printf -- '- Time (UTC): `%s`\n' "$(date -u '+%Y-%m-%d %H:%M:%S UTC' 2>/dev/null || printf 'unknown')"
printf -- '- OS: `%s`\n' "$(uname -srm 2>/dev/null || printf 'unknown')"
printf -- '- Architecture: `%s`\n' "$(uname -m 2>/dev/null || printf 'unknown')"
printf -- '- Shell: `%s`\n' "${SHELL:-unknown}"
echo
echo "### Tooling"
if command -v shatter >/dev/null 2>&1; then
    printf -- '- Shatter: `%s`\n' "$(first_line shatter --version)"
else
    echo '- Shatter: `not found on PATH`'
fi
if command -v node >/dev/null 2>&1; then
    printf -- '- Node.js: `%s`\n' "$(first_line node --version)"
fi
if command -v go >/dev/null 2>&1; then
    printf -- '- Go: `%s`\n' "$(first_line go version)"
fi
if command -v rustc >/dev/null 2>&1; then
    printf -- '- rustc: `%s`\n' "$(first_line rustc --version)"
fi
if command -v cargo >/dev/null 2>&1; then
    printf -- '- cargo: `%s`\n' "$(first_line cargo --version)"
fi
echo
echo "### Project"
printf -- '- Working directory: `%s`\n' "$(pwd)"
if [[ -n "$RUN_DIR" ]]; then
    printf -- '- Run directory: `%s`\n' "$RUN_DIR"
fi
if git rev-parse --show-toplevel >/dev/null 2>&1; then
    printf -- '- Git root: `%s`\n' "$(git rev-parse --show-toplevel)"
    printf -- '- Git branch: `%s`\n' "$(git branch --show-current 2>/dev/null || printf 'detached')"
    printf -- '- Git commit: `%s`\n' "$(git rev-parse HEAD 2>/dev/null || printf 'unknown')"
    git_status_lines="$(git status --short 2>/dev/null | sed -n '1,20p')"
    if [[ -n "$git_status_lines" ]]; then
        echo "- Git status summary:"
        echo
        echo '```text'
        printf '%s\n' "$git_status_lines"
        echo '```'
    else
        echo "- Git status summary: clean"
    fi
else
    echo "- Git: not a repository"
fi
echo
echo "### Shatter configuration"
found_config=false
if print_config_file "$ROOT_DIR/.shatter/config.yaml"; then
    found_config=true
fi
if print_config_file "$ROOT_DIR/shatter.scope.yaml"; then
    found_config=true
fi
for setup_file in "$ROOT_DIR"/shatter.setup.* "$ROOT_DIR"/.shatter/setup.*; do
    if [[ -e "$setup_file" ]]; then
        print_config_file "$setup_file"
        found_config=true
    fi
done
for target in "${TARGETS[@]}"; do
    target_path="${target%%:*}"
    target_dir="$(dirname "$target_path")"
    target_base="$(basename "$target_path")"
    target_stem="${target_base%.*}"
    for setup_file in "$target_dir/$target_stem".shatter.setup.*; do
        if [[ -e "$setup_file" ]]; then
            print_config_file "$setup_file"
            found_config=true
        fi
    done
done
if [[ "$found_config" == false ]]; then
    echo "- No Shatter config files detected."
fi
echo
if [[ ${#TARGETS[@]} -gt 0 ]]; then
    echo "### Targets"
    for target in "${TARGETS[@]}"; do
        print_file_item "${target%%:*}"
    done
    echo
fi
if [[ ${#ARTIFACTS[@]} -gt 0 ]]; then
    echo "### Artifacts"
    for artifact in "${ARTIFACTS[@]}"; do
        print_file_item "$artifact"
    done
    echo
fi
