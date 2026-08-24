#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SETUP_HOOKS="$REPO_ROOT/scripts/setup-hooks.sh"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

TEST_REPO="$SCRATCH/repo"
mkdir -p "$TEST_REPO/scripts"
git init -q "$TEST_REPO"
cp "$SETUP_HOOKS" "$TEST_REPO/scripts/setup-hooks.sh"
chmod +x "$TEST_REPO/scripts/setup-hooks.sh"

HOOKS=(pre-commit post-merge pre-push post-checkout prepare-commit-msg)
for hook in "${HOOKS[@]}"; do
    hook_file="$TEST_REPO/.git/hooks/$hook"
    cat > "$hook_file" <<EOF
#!/usr/bin/env sh
printf '%s=%s\n' '$hook' "\${BEADS_HOOK_TIMEOUT:-unset}" >> "\$HOOK_OUTPUT"
EOF
    chmod +x "$hook_file"
done

(cd "$TEST_REPO" && scripts/setup-hooks.sh)
(cd "$TEST_REPO" && scripts/setup-hooks.sh --check)

BEFORE="$SCRATCH/before.sha256"
AFTER="$SCRATCH/after.sha256"
sha256sum "${HOOKS[@]/#/$TEST_REPO/.git/hooks/}" > "$BEFORE"
(cd "$TEST_REPO" && scripts/setup-hooks.sh)
sha256sum "${HOOKS[@]/#/$TEST_REPO/.git/hooks/}" > "$AFTER"
cmp "$BEFORE" "$AFTER"

OUTPUT="$SCRATCH/hooks.out"
for hook in "${HOOKS[@]}"; do
    (cd "$TEST_REPO" && HOOK_OUTPUT="$OUTPUT" ".git/hooks/$hook" </dev/null)
done
for hook in "${HOOKS[@]}"; do
    grep -qx "$hook=30" "$OUTPUT"
done

: > "$OUTPUT"
(cd "$TEST_REPO" && HOOK_OUTPUT="$OUTPUT" BEADS_HOOK_TIMEOUT=7 ".git/hooks/post-checkout")
grep -qx 'post-checkout=7' "$OUTPUT"

for hook in "${HOOKS[@]}"; do
    hook_file="$TEST_REPO/.git/hooks/$hook"
    env_line="$(grep -n 'export BEADS_HOOK_TIMEOUT=' "$hook_file" | cut -d: -f1)"
    body_line="$(grep -n "printf '%s=%s" "$hook_file" | cut -d: -f1)"
    if [[ -z "$env_line" || "$env_line" -ge "$body_line" ]]; then
        echo "[FAIL] $hook timeout environment must precede existing hook content" >&2
        exit 1
    fi
done

echo "[ok] setup-hooks installs an ordered, idempotent 30s Beads timeout for every hook"
