#!/bin/sh
# Entrypoint for the Shatter container image (str-umw3).
#
# Ensures the working directory has a writable .shatter/ scratch directory
# under whatever the user bind-mounted (typically /work), then execs shatter
# with the user's arguments. Preserves stdio so logs and reports stream back
# unchanged.
#
# When invoked with --user "$(id -u):$(id -g)" the process runs as the
# host user's UID/GID, so all files written into bind-mounted volumes are
# owned by the host user — no root-owned artifacts to clean up.
set -eu

# mkdir -p tolerates pre-existing dirs and read-only mounts where the dir
# already exists; only fail if creation is genuinely needed and impossible.
if [ ! -d .shatter ]; then
    mkdir -p .shatter 2>/dev/null || true
fi

# When running as an arbitrary UID (via --user), the UID may not exist in
# /etc/passwd. Some tools (git, npm) refuse to run without a passwd entry.
# Create a minimal entry so they don't break.
if ! whoami >/dev/null 2>&1; then
    echo "shatter:x:$(id -u):$(id -g):shatter:/tmp:/bin/sh" >> /etc/passwd 2>/dev/null || true
fi

exec shatter "$@"
