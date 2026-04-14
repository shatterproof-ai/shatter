#!/bin/sh
# Entrypoint for the Shatter container image (str-umw3).
#
# Ensures the working directory has writable output directories under
# whatever the user bind-mounted (typically /work), then execs shatter
# with the user's arguments. Preserves stdio so logs and reports stream
# back unchanged.
#
# When invoked with --user "$(id -u):$(id -g)" the process runs as the
# host user's UID/GID, so all files written into bind-mounted volumes are
# owned by the host user — no root-owned artifacts to clean up.
#
# Split-mount mode: mount the source tree read-only and only the output
# paths read-write. See docker/README.md for the canonical invocation.
set -eu

# Pre-create each output directory Shatter may write to.  mkdir -p
# tolerates pre-existing dirs; suppress errors for read-only mounts where
# the directory already exists.
for dir in .shatter .shatter-cache shatter-artifacts; do
    if [ ! -d "$dir" ]; then
        mkdir -p "$dir" 2>/dev/null || true
    fi
done

# When running as an arbitrary UID (via --user), the UID may not exist in
# /etc/passwd. Some tools (git, npm) refuse to run without a passwd entry.
# Create a minimal entry so they don't break.
if ! whoami >/dev/null 2>&1; then
    echo "shatter:x:$(id -u):$(id -g):shatter:/tmp:/bin/sh" >> /etc/passwd 2>/dev/null || true
fi

exec shatter "$@"
