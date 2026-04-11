#!/bin/sh
# Entrypoint for the Shatter container image (str-umw3).
#
# Ensures the working directory has a writable .shatter/ scratch directory
# under whatever the user bind-mounted (typically /work), then execs shatter
# with the user's arguments. Preserves stdio so logs and reports stream back
# unchanged.
set -eu

# mkdir -p tolerates pre-existing dirs and read-only mounts where the dir
# already exists; only fail if creation is genuinely needed and impossible.
if [ ! -d .shatter ]; then
    mkdir -p .shatter 2>/dev/null || true
fi

exec shatter "$@"
