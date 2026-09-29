#!/usr/bin/env bash
# Synthetic tests for the staged private-key commit guard (str-0znjr):
# scripts/check-staged-private-keys.py wired into the pre-commit hook that
# scripts/setup-hooks.sh installs.
#
# Every fixture is a throwaway git repo with isolated git env (see
# scripts/git-sandbox-test-lib.sh). All "keys" are synthetic, non-credential
# text: a BEGIN line, a body carrying a recognizable marker, an END line. No
# real credential is read or written. The marker must never appear in the
# hook's diagnostics.
#
# Usage: bash scripts/test_staged_private_key_guard.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$REPO_ROOT/scripts/git-sandbox-test-lib.sh"

FIXTURE="$(mktemp -d)"
trap 'rm -rf "$FIXTURE"' EXIT

FAIL=0
fail() {
    echo "[FAIL] $1" >&2
    FAIL=1
}

MARKER="SYNTHETICBODYMARKER0123456789"
DASHES="-----"

# synth_key <label>  (label may be empty: generic "PRIVATE KEY")
synth_key() {
    local label="$1" sp=" "
    [ -n "$label" ] || sp=""
    printf '%sBEGIN %s%sPRIVATE KEY%s\n' "$DASHES" "$label" "$sp" "$DASHES"
    printf '%s\n%s\n' "$MARKER" "c3ludGhldGljLW5vbmNyZWRlbnRpYWw="
    printf '%sEND %s%sPRIVATE KEY%s\n' "$DASHES" "$label" "$sp" "$DASHES"
}

# --- Fixture repository -------------------------------------------------
mkdir -p "$FIXTURE/scripts" "$FIXTURE/bin"
git init -q --initial-branch=scratch "$FIXTURE"
git -C "$FIXTURE" config user.email key-guard-test@example.test
git -C "$FIXTURE" config user.name "Key Guard Test"
git -C "$FIXTURE" config core.hooksPath "$FIXTURE/.git/hooks"
git -C "$FIXTURE" config commit.gpgsign false

cp "$REPO_ROOT/scripts/setup-hooks.sh" "$FIXTURE/scripts/setup-hooks.sh"
cp "$REPO_ROOT/scripts/check-staged-private-keys.py" "$FIXTURE/scripts/check-staged-private-keys.py"
# Stub Rust checker: succeeds, so any rejection comes from the key guard.
printf '#!/usr/bin/env bash\nexit 0\n' > "$FIXTURE/scripts/precommit-rust.sh"
chmod +x "$FIXTURE/scripts/precommit-rust.sh"
printf '.agentsea/\n' > "$FIXTURE/.gitignore"

# Pre-existing unrelated hook content (Beads-style) that must survive.
cat > "$FIXTURE/.git/hooks/pre-commit" <<'EOF'
#!/usr/bin/env sh
echo "SENTINEL_PREEXISTING_HOOK_RAN"
EOF
chmod +x "$FIXTURE/.git/hooks/pre-commit"

bash "$FIXTURE/scripts/setup-hooks.sh" >/dev/null
HOOK_AFTER_FIRST="$(cat "$FIXTURE/.git/hooks/pre-commit")"
bash "$FIXTURE/scripts/setup-hooks.sh" >/dev/null
if [ "$HOOK_AFTER_FIRST" != "$(cat "$FIXTURE/.git/hooks/pre-commit")" ]; then
    fail "setup-hooks is not idempotent for pre-commit"
fi
grep -q SENTINEL_PREEXISTING_HOOK_RAN "$FIXTURE/.git/hooks/pre-commit" ||
    fail "pre-existing hook content was not preserved"
[ "$(grep -c 'BEGIN SHATTER QUALITY' "$FIXTURE/.git/hooks/pre-commit")" = 1 ] ||
    fail "expected exactly one SHATTER QUALITY section"
bash "$FIXTURE/scripts/setup-hooks.sh" --check >/dev/null ||
    fail "setup-hooks --check reports the fresh install as missing"

echo base > "$FIXTURE/README.md"
git -C "$FIXTURE" add README.md .gitignore scripts
git -C "$FIXTURE" commit -q -m base >/dev/null 2>&1 || fail "baseline commit failed"

# try_commit: runs a real `git commit`; sets RC and OUT.
try_commit() {
    RC=0
    OUT="$(git -C "$FIXTURE" commit -q -m "$1" 2>&1)" || RC=$?
}

expect_rejected() { # <label> <type-substring> <path-substring>
    if [ "$RC" -eq 0 ]; then fail "$1: commit should have been rejected"; fi
    case "$OUT" in *"$2"*) ;; *) fail "$1: diagnostics missing type '$2': $OUT" ;; esac
    case "$OUT" in *"$3"*) ;; *) fail "$1: diagnostics missing path '$3': $OUT" ;; esac
    case "$OUT" in *"$MARKER"*) fail "$1: diagnostics leaked key body" ;; esac
    case "$OUT" in *"c3ludGhldGljLW5vbmNyZWRlbnRpYWw="*) fail "$1: diagnostics leaked key body" ;; esac
}

reset_index() {
    git -C "$FIXTURE" reset -q --hard HEAD
    git -C "$FIXTURE" clean -qfdx -e bin
}

# 1. Force-staged key under an ignored path.
mkdir -p "$FIXTURE/.agentsea/ssh"
synth_key OPENSSH > "$FIXTURE/.agentsea/ssh/deploy_key"
git -C "$FIXTURE" add -f .agentsea/ssh/deploy_key
try_commit force-added
expect_rejected "force-added ignored path" "OPENSSH PRIVATE KEY" ".agentsea/ssh/deploy_key"
case "$OUT" in *SENTINEL_PREEXISTING_HOOK_RAN*) ;; *) fail "pre-existing hook did not run before rejection" ;; esac

# 2. Another key at an ordinary path (with the first still staged).
mkdir -p "$FIXTURE/notes"
synth_key RSA > "$FIXTURE/notes/config.txt"
git -C "$FIXTURE" add notes/config.txt
try_commit ordinary
expect_rejected "ordinary path" "RSA PRIVATE KEY" "notes/config.txt"
reset_index

# 3. Staged key, working file replaced with clean content: index still checked.
synth_key "EC" > "$FIXTURE/mismatch.txt"
git -C "$FIXTURE" add mismatch.txt
echo "clean now" > "$FIXTURE/mismatch.txt"
try_commit mismatch
expect_rejected "staged/working mismatch" "EC PRIVATE KEY" "mismatch.txt"
reset_index

# 4. Other key variants, regardless of extension.
for label in "" "ENCRYPTED"; do
    synth_key "$label" > "$FIXTURE/variant.bin"
    git -C "$FIXTURE" add variant.bin
    try_commit variant
    want="PRIVATE KEY"
    [ -z "$label" ] || want="$label PRIVATE KEY"
    expect_rejected "variant '${label:-generic}'" "$want" "variant.bin"
    reset_index
done

# 5. Newline-containing path, and CRLF-terminated key block.
NLPATH=$'weird\nname.txt'
synth_key OPENSSH > "$FIXTURE/$NLPATH"
git -C "$FIXTURE" add -- "$NLPATH"
try_commit newline-path
expect_rejected "newline path" "OPENSSH PRIVATE KEY" "weird"
if [ "$(printf '%s\n' "$OUT" | grep -c '^name.txt')" -ne 0 ]; then
    fail "newline path was emitted raw, splitting a diagnostic line"
fi
reset_index
synth_key RSA | sed 's/$/\r/' > "$FIXTURE/crlf.txt"
git -C "$FIXTURE" add crlf.txt
try_commit crlf
expect_rejected "CRLF block" "RSA PRIVATE KEY" "crlf.txt"
reset_index

# 6. Controls that must pass.
printf 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIsyntheticpublickeyonly user@example.test\n' > "$FIXTURE/id.pub"
printf 'Supported: %sBEGIN RSA PRIVATE KEY%s blocks are rejected by the hook.\n' "$DASHES" "$DASHES" > "$FIXTURE/doc.md"
printf '%sBEGIN OPENSSH PRIVATE KEY%s\n%s\n' "$DASHES" "$DASHES" "$MARKER" > "$FIXTURE/truncated.txt"
git -C "$FIXTURE" add id.pub doc.md truncated.txt
synth_key OPENSSH > "$FIXTURE/unstaged-only.txt" # untracked, never staged
try_commit controls
if [ "$RC" -ne 0 ]; then fail "controls (public key, doc sentence, truncated, unstaged-only) were rejected: $OUT"; fi
reset_index

# 7. Git/blob read failures fail closed. A `git` shim delegates everything
# except the named subcommand, which exits nonzero.
REAL_GIT="$(command -v git)"
for broken in cat-file diff; do
    cat > "$FIXTURE/bin/git" <<EOF
#!/bin/sh
for a in "\$@"; do
  if [ "\$a" = "$broken" ]; then echo "simulated git failure" >&2; exit 1; fi
done
exec "$REAL_GIT" "\$@"
EOF
    chmod +x "$FIXTURE/bin/git"
    echo "clean" > "$FIXTURE/readfail.txt"
    git -C "$FIXTURE" add readfail.txt
    RC=0
    OUT="$(cd "$FIXTURE" && PATH="$FIXTURE/bin:$PATH" python3 scripts/check-staged-private-keys.py 2>&1)" || RC=$?
    if [ "$RC" -eq 0 ]; then fail "git $broken failure did not fail closed"; fi
    reset_index
done
rm -f "$FIXTURE/bin/git"

if [ "$FAIL" -ne 0 ]; then
    echo "[FAIL] test_staged_private_key_guard: one or more assertions failed" >&2
    exit 1
fi
echo "[ok] test_staged_private_key_guard: all scenarios passed"
