#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$REPO_ROOT/scripts/target-dir-report.sh"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

WORKTREE_ONE="$SCRATCH/worktree-one"
WORKTREE_TWO="$SCRATCH/worktree-two"
mkdir -p "$WORKTREE_ONE/target" "$WORKTREE_ONE/shatter-rust/target"
mkdir -p "$WORKTREE_TWO/shatter-rust-runtime/target"
dd if=/dev/zero of="$WORKTREE_ONE/target/root.bin" bs=1024 count=4 status=none
dd if=/dev/zero of="$WORKTREE_ONE/shatter-rust/target/frontend.bin" bs=1024 count=8 status=none
dd if=/dev/zero of="$WORKTREE_TWO/shatter-rust-runtime/target/runtime.bin" bs=1024 count=16 status=none

OUTPUT="$("$SCRIPT" --worktree "$WORKTREE_ONE" --worktree "$WORKTREE_TWO")"

for EXPECTED in \
    "$WORKTREE_ONE/target" \
    "$WORKTREE_ONE/shatter-rust/target" \
    "$WORKTREE_TWO/shatter-rust-runtime/target" \
    "TOTAL"; do
    if ! printf '%s\n' "$OUTPUT" | grep -Fq "$EXPECTED"; then
        echo "[FAIL] target-dir report missing: $EXPECTED" >&2
        printf '%s\n' "$OUTPUT" >&2
        exit 1
    fi
done

REPORTED_TOTAL="$(printf '%s\n' "$OUTPUT" | awk -F '\t' '$3 == "TOTAL" { print $1 }')"
CALCULATED_TOTAL="$(printf '%s\n' "$OUTPUT" | awk -F '\t' '$3 != "TARGET_DIR" && $3 != "TOTAL" { sum += $1 } END { print sum }')"
if [[ "$REPORTED_TOTAL" != "$CALCULATED_TOTAL" ]]; then
    echo "[FAIL] reported total must equal the sum of target-directory sizes" >&2
    printf '%s\n' "$OUTPUT" >&2
    exit 1
fi

mkdir -p "$SCRATCH/bin"
cat > "$SCRATCH/bin/du" <<EOF
#!/usr/bin/env bash
if [[ "\${*: -1}" == "$WORKTREE_ONE/target" ]]; then
    exit 1
fi
exec /usr/bin/du "\$@"
EOF
chmod +x "$SCRATCH/bin/du"

if ! OUTPUT_WITH_RACE="$(PATH="$SCRATCH/bin:$PATH" "$SCRIPT" --worktree "$WORKTREE_ONE" --worktree "$WORKTREE_TWO" 2>&1)"; then
    echo "[FAIL] one unreadable or disappearing target must not abort the report" >&2
    exit 1
fi
if ! printf '%s\n' "$OUTPUT_WITH_RACE" | grep -Fq "$WORKTREE_TWO/shatter-rust-runtime/target"; then
    echo "[FAIL] report must continue after one target-directory error" >&2
    exit 1
fi

echo "[ok] target-dir-report lists target directories across worktrees"
