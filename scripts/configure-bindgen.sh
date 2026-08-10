#!/usr/bin/env bash
# Auto-detect GCC include path for bindgen (used by z3-sys).
#
# When only libclang1-XX is installed (without libclang-dev), clang's builtin
# headers (stdbool.h, etc.) are missing. Bindgen needs them, so we point it to
# GCC's copy via BINDGEN_EXTRA_CLANG_ARGS.
#
# This script manages a marked block in the MACHINE-level cargo config
# (${CARGO_HOME:-~/.cargo}/config.toml). It must not touch the repo's
# .cargo/config.toml — that file is tracked and carries the shared-machine
# parallelism caps (str-35vtk.5). Cargo merges both configs; the repo file
# wins per-key, and the two set disjoint keys.
#
# Run once after cloning (or after changing GCC versions). Idempotent.
# Not needed when libclang-dev is installed (the block is removed then).

set -euo pipefail

CONFIG_DIR="${CARGO_HOME:-$HOME/.cargo}"
CONFIG_FILE="$CONFIG_DIR/config.toml"
MARK_BEGIN="# >>> shatter bindgen workaround (managed by configure-bindgen.sh)"
MARK_END="# <<< shatter bindgen workaround"

remove_block() {
    if [ -f "$CONFIG_FILE" ] && grep -qF "$MARK_BEGIN" "$CONFIG_FILE"; then
        tmp="$(mktemp "$CONFIG_DIR/config.toml.XXXXXX")"
        awk -v b="$MARK_BEGIN" -v e="$MARK_END" \
            '$0==b{skip=1} !skip{print} $0==e{skip=0}' \
            "$CONFIG_FILE" > "$tmp"
        mv "$tmp" "$CONFIG_FILE"
        return 0
    fi
    return 1
}

# Check if libclang-dev is installed (provides clang's own headers)
if dpkg -l libclang-dev &>/dev/null 2>&1; then
    if remove_block; then
        echo "Removed bindgen workaround block from $CONFIG_FILE (libclang-dev provides clang headers)"
    else
        echo "No bindgen workaround needed (libclang-dev is installed)"
    fi
    exit 0
fi

# Detect GCC include path
GCC_INCLUDE="$(gcc -print-file-name=include 2>/dev/null || true)"

if [ -z "$GCC_INCLUDE" ] || [ ! -d "$GCC_INCLUDE" ]; then
    echo >&2 "Warning: could not detect GCC include path."
    echo >&2 "Install libclang-dev or GCC and re-run this script."
    exit 1
fi

mkdir -p "$CONFIG_DIR"
remove_block || true

# A TOML file may declare [env] only once; appending a fresh [env] header in
# the marked block is safe only if the file doesn't already have one outside
# the block. Detect and reuse an existing [env] table instead of duplicating.
#
# The detection regex and the awk insertion pattern below must accept the
# exact same set of lines as an "[env]" header — a header with trailing
# whitespace or a CRLF line ending previously matched detection (loose
# grep) but not insertion (exact-string awk match), so the workaround was
# silently never written even though the script printed success and exited
# 0 (review follow-up). Both now tolerate trailing whitespace/CR, and the
# awk pass verifies it actually inserted the block rather than trusting the
# detection regex blindly.
#
# `\t`/`\r` are gawk regex escapes but NOT POSIX ERE escapes — GNU grep -E
# (unlike -P) treats a bare `\t`/`\r` as a literal "t"/"r", so embed the real
# tab/CR bytes via $'...' instead of relying on grep to interpret them.
tab=$(printf '\t')
cr=$(printf '\r')
env_header_re="^\\[env\\][ ${tab}]*${cr}?\$"
if [ -f "$CONFIG_FILE" ] && grep -qE "$env_header_re" "$CONFIG_FILE"; then
    tmp="$(mktemp "$CONFIG_DIR/config.toml.XXXXXX")"
    if ! awk -v b="$MARK_BEGIN" -v e="$MARK_END" -v inc="$GCC_INCLUDE" '
        {print}
        /^\[env\][ \t]*\r?$/ && !done {
            print b
            print "BINDGEN_EXTRA_CLANG_ARGS = { value = \"-I" inc "\", force = false }"
            print e
            done=1
        }
        END { if (!done) exit 1 }' "$CONFIG_FILE" > "$tmp"; then
        rm -f "$tmp"
        echo "error: found an [env] table in $CONFIG_FILE but could not insert the bindgen workaround next to it" >&2
        echo "  add this line under [env] manually:" >&2
        echo "  BINDGEN_EXTRA_CLANG_ARGS = { value = \"-I${GCC_INCLUDE}\", force = false }" >&2
        exit 1
    fi
    mv "$tmp" "$CONFIG_FILE"
else
    {
        echo "$MARK_BEGIN"
        echo "[env]"
        echo "BINDGEN_EXTRA_CLANG_ARGS = { value = \"-I${GCC_INCLUDE}\", force = false }"
        echo "$MARK_END"
    } >> "$CONFIG_FILE"
fi

echo "Configured bindgen workaround in $CONFIG_FILE (GCC includes: $GCC_INCLUDE)"
