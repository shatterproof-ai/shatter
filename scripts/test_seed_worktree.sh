#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$REPO_ROOT/scripts/seed-worktree.sh"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

CANONICAL="$SCRATCH/canonical"
TARGET="$SCRATCH/target"
mkdir -p "$CANONICAL/shatter-ts/node_modules/example" "$TARGET/shatter-ts"
printf '{"lockfileVersion": 3}\n' > "$CANONICAL/shatter-ts/package-lock.json"
cp "$CANONICAL/shatter-ts/package-lock.json" "$TARGET/shatter-ts/package-lock.json"
printf 'seeded\n' > "$CANONICAL/shatter-ts/node_modules/example/index.js"

"$SCRIPT" "$TARGET" "$CANONICAL"

SOURCE_INODE="$(stat -c '%i' "$CANONICAL/shatter-ts/node_modules/example/index.js")"
TARGET_INODE="$(stat -c '%i' "$TARGET/shatter-ts/node_modules/example/index.js")"
if [[ "$SOURCE_INODE" != "$TARGET_INODE" ]]; then
    echo "[FAIL] matching lockfiles must hardlink node_modules" >&2
    exit 1
fi

rm -rf "$TARGET/shatter-ts/node_modules"
mkdir -p "$SCRATCH/autodetect-bin"
cat > "$SCRATCH/autodetect-bin/git" <<EOF
#!/usr/bin/env bash
set -euo pipefail
[[ "\$*" == "worktree list --porcelain" ]]
printf '%s\n' \
    "worktree $CANONICAL" \
    "HEAD 1111111111111111111111111111111111111111" \
    "branch refs/heads/main" \
    "" \
    "worktree $TARGET" \
    "HEAD 2222222222222222222222222222222222222222" \
    "branch refs/heads/feature"
EOF
chmod +x "$SCRATCH/autodetect-bin/git"

PATH="$SCRATCH/autodetect-bin:$PATH" "$SCRIPT" "$TARGET"

AUTODETECT_INODE="$(stat -c '%i' "$TARGET/shatter-ts/node_modules/example/index.js")"
if [[ "$SOURCE_INODE" != "$AUTODETECT_INODE" ]]; then
    echo "[FAIL] omitted canonical path must detect the main worktree" >&2
    exit 1
fi

rm -rf "$TARGET/shatter-ts/node_modules"
printf '{"lockfileVersion": 2}\n' > "$TARGET/shatter-ts/package-lock.json"
mkdir -p "$SCRATCH/bin"
cat > "$SCRATCH/bin/npm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == "ci" ]]
mkdir -p node_modules
printf '%s\n' "$PWD" > node_modules/npm-ci-ran
EOF
chmod +x "$SCRATCH/bin/npm"

PATH="$SCRATCH/bin:$PATH" "$SCRIPT" "$TARGET" "$CANONICAL"

EXPECTED="$TARGET/shatter-ts"
ACTUAL="$(<"$TARGET/shatter-ts/node_modules/npm-ci-ran")"
if [[ "$ACTUAL" != "$EXPECTED" ]]; then
    echo "[FAIL] mismatched lockfiles must run npm ci in the target worktree" >&2
    exit 1
fi

rm -rf "$TARGET/shatter-ts/node_modules"
cp "$CANONICAL/shatter-ts/package-lock.json" "$TARGET/shatter-ts/package-lock.json"
cat > "$SCRATCH/bin/cp" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "-al" ]]; then
    exit 18
fi
exec /usr/bin/cp "$@"
EOF
chmod +x "$SCRATCH/bin/cp"

PATH="$SCRATCH/bin:$PATH" "$SCRIPT" "$TARGET" "$CANONICAL"

ACTUAL="$(<"$TARGET/shatter-ts/node_modules/npm-ci-ran")"
if [[ "$ACTUAL" != "$EXPECTED" ]]; then
    echo "[FAIL] failed hardlink clone must fall back to npm ci" >&2
    exit 1
fi

exec 9>"$TARGET/shatter-ts/.seed-worktree.lock"
flock -n 9
if PATH="$SCRATCH/bin:$PATH" "$SCRIPT" "$TARGET" "$CANONICAL" 2>/dev/null; then
    echo "[FAIL] an existing seed lock must reject a concurrent invocation" >&2
    exit 1
fi
flock -u 9
exec 9>&-

echo "[ok] seed-worktree detects main, hardlinks matching dependencies, and falls back to npm ci"
