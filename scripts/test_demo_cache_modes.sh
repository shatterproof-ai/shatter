#!/usr/bin/env bash

# Regression contract for the walkthrough/gauntlet cache lifecycle (str-35vtk.6).
# The fake Python executable records the exported environment at the exact
# examples-checkout boundary, then fails deliberately so the scripts run their
# real EXIT cleanup without cloning examples or executing the demo.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

REAL_PYTHON="$(command -v python3)"
REAL_MKTEMP="$(command -v mktemp)"
FAKE_BIN="$SCRATCH/bin"
mkdir -p "$FAKE_BIN"

cat > "$FAKE_BIN/python3" <<'PYTHON'
#!/usr/bin/env bash
set -euo pipefail

if [[ $# -eq 2 && "$1" == "$CHECKPOINT_HELPER" && "$2" == "--fresh" ]]; then
    : "${CHECKPOINT_CAPTURE:?}"
    for name in \
        SHATTER_CACHE_DIR SHATTER_SEEDS_DIR SHATTER_HARNESS_CACHE \
        SHATTER_ARTIFACT_DIR CARGO_TARGET_DIR XDG_CACHE_HOME GOCACHE; do
        if [[ -v "$name" ]]; then
            printf '%s=%s\n' "$name" "${!name}" >> "$CHECKPOINT_CAPTURE"
        else
            printf '%s=<unset>\n' "$name" >> "$CHECKPOINT_CAPTURE"
        fi
    done

    if [[ "${CHECKPOINT_TOUCH:-0}" == 1 ]]; then
        for name in SHATTER_CACHE_DIR SHATTER_HARNESS_CACHE SHATTER_ARTIFACT_DIR CARGO_TARGET_DIR XDG_CACHE_HOME GOCACHE; do
            if [[ -v "$name" && -n "${!name}" ]]; then
                mkdir -p "${!name}"
                touch "${!name}/checkpoint-marker"
            fi
        done
    fi

    if [[ -n "${CHECKPOINT_REACHED:-}" ]]; then
        touch "$CHECKPOINT_REACHED"
    fi
    if [[ -n "${CHECKPOINT_RELEASE:-}" ]]; then
        while [[ ! -e "$CHECKPOINT_RELEASE" ]]; do
            sleep 0.02
        done
    fi
    exit 86
fi

exec "$REAL_PYTHON" "$@"
PYTHON

cat > "$FAKE_BIN/mktemp" <<'MKTEMP'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$MKTEMP_LOG"
exec "$REAL_MKTEMP" "$@"
MKTEMP

cat > "$FAKE_BIN/shatter" <<'SHATTER'
#!/usr/bin/env bash
exit 0
SHATTER

cat > "$FAKE_BIN/shatter-rust" <<'RUST_FRONTEND'
#!/usr/bin/env bash
exit 0
RUST_FRONTEND

chmod +x "$FAKE_BIN/python3" "$FAKE_BIN/mktemp" \
    "$FAKE_BIN/shatter" "$FAKE_BIN/shatter-rust"

export REAL_PYTHON REAL_MKTEMP
export PATH="$FAKE_BIN:$PATH"
export CHECKPOINT_HELPER="$REPO_ROOT/scripts/examples_checkout.py"
export SHATTER_BIN="$FAKE_BIN/shatter"
export SHATTER_RUST_FRONTEND="$FAKE_BIN/shatter-rust"
export MKTEMP_LOG="$SCRATCH/mktemp.log"

FAILURES=0

fail() {
    echo "[FAIL] $*" >&2
    FAILURES=$((FAILURES + 1))
}

value_of() {
    local capture="$1" name="$2"
    if [[ ! -f "$capture" ]]; then
        printf '<missing capture>\n'
        return
    fi
    awk -F= -v name="$name" '$1 == name { sub(/^[^=]*=/, ""); print; exit }' "$capture"
}

check_eq() {
    local actual="$1" expected="$2" message="$3"
    [[ "$actual" == "$expected" ]] || fail "$message (expected '$expected', got '$actual')"
}

check_exists() {
    [[ -e "$1" ]] || fail "$2 ($1)"
}

check_absent() {
    [[ ! -e "$1" ]] || fail "$2 ($1)"
}

check_under() {
    local path="$1" root="$2" message="$3"
    [[ "$path" == "$root"/* ]] || fail "$message ('$path' is not under '$root')"
}

reset_demo_env() {
    unset SHATTER_DEMO_CACHE SHATTER_CACHE_DIR SHATTER_SEEDS_DIR
    unset SHATTER_HARNESS_CACHE SHATTER_ARTIFACT_DIR CARGO_TARGET_DIR
    unset XDG_CACHE_HOME GOCACHE
    unset CHECKPOINT_REACHED CHECKPOINT_RELEASE
    export HOME="$SCRATCH/home fallback"
    export TMPDIR="$SCRATCH/tmp"
    mkdir -p "$HOME" "$TMPDIR"
}

run_to_checkpoint() {
    local script="$1" capture="$2"
    shift 2
    : > "$MKTEMP_LOG"
    rm -f "$capture"
    export CHECKPOINT_CAPTURE="$capture" CHECKPOINT_TOUCH=1
    if bash "$REPO_ROOT/demo/$script.sh" "$@" \
        > "$capture.stdout" 2> "$capture.stderr"; then
        fail "$script unexpectedly succeeded past the forced checkout failure"
    fi
    [[ -f "$capture" ]] || fail "$script did not reach the exact examples checkout checkpoint"
}

assert_warm_paths() {
    local capture="$1" warm_root="$2" label="$3"
    check_eq "$(value_of "$capture" SHATTER_CACHE_DIR)" "$warm_root/cache" "$label cache path"
    check_eq "$(value_of "$capture" SHATTER_HARNESS_CACHE)" "$warm_root/harness" "$label harness path"
    check_eq "$(value_of "$capture" CARGO_TARGET_DIR)" "$warm_root/cargo-target" "$label cargo path"
}

echo "[test] warm defaults use stable HOME fallback and preserve host caches"
reset_demo_env
warm_root="$HOME/.cache/shatter-demo"
host_xdg="$SCRATCH/host xdg"
host_go="$SCRATCH/host go"
mkdir -p "$host_xdg" "$host_go"
touch "$host_xdg/caller-marker" "$host_go/caller-marker"
export XDG_CACHE_HOME="$host_xdg" GOCACHE="$host_go"

for script in walkthrough gauntlet; do
    first="$SCRATCH/$script-warm-1.env"
    second="$SCRATCH/$script-warm-2.env"
    run_to_checkpoint "$script" "$first"
    run_to_checkpoint "$script" "$second"
    assert_warm_paths "$first" "$warm_root" "$script warm run 1"
    assert_warm_paths "$second" "$warm_root" "$script warm run 2"
    check_eq "$(value_of "$first" SHATTER_CACHE_DIR)" \
        "$(value_of "$second" SHATTER_CACHE_DIR)" "$script warm cache must be stable"
    check_eq "$(value_of "$first" XDG_CACHE_HOME)" "$host_xdg" "$script must inherit warm XDG_CACHE_HOME"
    check_eq "$(value_of "$first" GOCACHE)" "$host_go" "$script must inherit warm GOCACHE"
    check_exists "$warm_root/cache/checkpoint-marker" "$script warm cache must survive exit"
    check_exists "$warm_root/harness/checkpoint-marker" "$script warm harness cache must survive exit"
    check_exists "$warm_root/cargo-target/checkpoint-marker" "$script warm cargo cache must survive exit"
    if [[ "$script" == gauntlet ]]; then
        first_artifact="$(value_of "$first" SHATTER_ARTIFACT_DIR)"
        second_artifact="$(value_of "$second" SHATTER_ARTIFACT_DIR)"
        [[ "$first_artifact" != "$second_artifact" ]] || fail "gauntlet warm artifacts must be per-run"
        check_absent "$first_artifact" "gauntlet warm artifact run 1 must be cleaned"
        check_absent "$second_artifact" "gauntlet warm artifact run 2 must be cleaned"
    else
        check_eq "$(value_of "$first" SHATTER_ARTIFACT_DIR)" "<unset>" \
            "walkthrough must not invent an artifact directory"
    fi
done
check_exists "$host_xdg/caller-marker" "warm run deleted caller XDG cache"
check_exists "$host_go/caller-marker" "warm run deleted caller Go cache"

echo "[test] SHATTER_DEMO_CACHE supports spaces and ambient warm overrides win"
reset_demo_env
export SHATTER_DEMO_CACHE="$SCRATCH/custom warm root"
ambient_cache="$SCRATCH/ambient cache"
ambient_harness="$SCRATCH/ambient harness"
ambient_artifact="$SCRATCH/ambient artifact"
ambient_cargo="$SCRATCH/ambient cargo"
mkdir -p "$ambient_cache" "$ambient_harness" "$ambient_artifact" "$ambient_cargo"
touch "$ambient_cache/caller-marker" "$ambient_harness/caller-marker" \
    "$ambient_artifact/caller-marker" "$ambient_cargo/caller-marker"
export SHATTER_CACHE_DIR="$ambient_cache"
export SHATTER_HARNESS_CACHE="$ambient_harness"
export SHATTER_ARTIFACT_DIR="$ambient_artifact"
export CARGO_TARGET_DIR="$ambient_cargo"
override_capture="$SCRATCH/gauntlet-warm-overrides.env"
run_to_checkpoint gauntlet "$override_capture"
check_eq "$(value_of "$override_capture" SHATTER_CACHE_DIR)" "$ambient_cache" "ambient cache must win"
check_eq "$(value_of "$override_capture" SHATTER_HARNESS_CACHE)" "$ambient_harness" "ambient harness must win"
check_eq "$(value_of "$override_capture" SHATTER_ARTIFACT_DIR)" "$ambient_artifact" "ambient artifact must win"
check_eq "$(value_of "$override_capture" CARGO_TARGET_DIR)" "$ambient_cargo" "ambient cargo must win"
for path in "$ambient_cache" "$ambient_harness" "$ambient_artifact" "$ambient_cargo"; do
    check_exists "$path/caller-marker" "warm cleanup deleted an ambient path"
done

# With only the root override, all warm defaults derive from that root.
unset SHATTER_CACHE_DIR SHATTER_HARNESS_CACHE SHATTER_ARTIFACT_DIR CARGO_TARGET_DIR
spaces_capture="$SCRATCH/walkthrough-warm-spaces.env"
run_to_checkpoint walkthrough "$spaces_capture"
assert_warm_paths "$spaces_capture" "$SHATTER_DEMO_CACHE" "walkthrough spaced warm root"

echo "[test] cold mode isolates and removes script-owned paths"
reset_demo_env
export SHATTER_DEMO_CACHE="$SCRATCH/warm-root-must-be-ignored"
for script in walkthrough gauntlet; do
    first="$SCRATCH/$script-cold-1.env"
    second="$SCRATCH/$script-cold-2.env"
    run_to_checkpoint "$script" "$first" --cold
    run_to_checkpoint "$script" "$second" --cold
    for capture in "$first" "$second"; do
        cache="$(value_of "$capture" SHATTER_CACHE_DIR)"
        cargo="$(value_of "$capture" CARGO_TARGET_DIR)"
        check_under "$cache" "$TMPDIR" "$script cold cache must be temporary"
        check_under "$cargo" "$TMPDIR" "$script cold cargo target must be temporary"
        check_absent "$cache" "$script cold cache must be cleaned"
        check_absent "$cargo" "$script cold cargo target must be cleaned"
        check_eq "$(value_of "$capture" SHATTER_HARNESS_CACHE)" "<unset>" \
            "$script cold mode must disable the warm harness cache"
        if [[ "$script" == gauntlet ]]; then
            artifact="$(value_of "$capture" SHATTER_ARTIFACT_DIR)"
            xdg="$(value_of "$capture" XDG_CACHE_HOME)"
            go_cache="$(value_of "$capture" GOCACHE)"
            check_under "$artifact" "$TMPDIR" "gauntlet cold artifact must be temporary"
            check_under "$xdg" "$TMPDIR" "gauntlet cold XDG cache must be temporary"
            check_under "$go_cache" "$TMPDIR" "gauntlet cold Go cache must be temporary"
            check_absent "$artifact" "gauntlet cold artifact must be cleaned"
            check_absent "$xdg" "gauntlet cold XDG cache must be cleaned"
            check_absent "$go_cache" "gauntlet cold Go cache must be cleaned"
        else
            check_eq "$(value_of "$capture" SHATTER_ARTIFACT_DIR)" "<unset>" \
                "walkthrough cold mode must not invent an artifact directory"
            check_eq "$(value_of "$capture" XDG_CACHE_HOME)" "<unset>" \
                "walkthrough cold mode must inherit unset XDG_CACHE_HOME"
            check_eq "$(value_of "$capture" GOCACHE)" "<unset>" \
                "walkthrough cold mode must inherit unset GOCACHE"
        fi
    done
    [[ "$(value_of "$first" SHATTER_CACHE_DIR)" != "$(value_of "$second" SHATTER_CACHE_DIR)" ]] || \
        fail "$script cold cache must differ between runs"
    [[ "$(value_of "$first" CARGO_TARGET_DIR)" != "$(value_of "$second" CARGO_TARGET_DIR)" ]] || \
        fail "$script cold cargo target must differ between runs"
done

echo "[test] cold cleanup preserves caller-owned Cargo, XDG, and Go caches"
reset_demo_env
caller_cargo="$SCRATCH/caller cargo"
caller_xdg="$SCRATCH/caller xdg"
caller_go="$SCRATCH/caller go"
mkdir -p "$caller_cargo" "$caller_xdg" "$caller_go"
touch "$caller_cargo/caller-marker" "$caller_xdg/caller-marker" "$caller_go/caller-marker"
export CARGO_TARGET_DIR="$caller_cargo" XDG_CACHE_HOME="$caller_xdg" GOCACHE="$caller_go"
for script in walkthrough gauntlet; do
    capture="$SCRATCH/$script-cold-callers.env"
    run_to_checkpoint "$script" "$capture" --cold
    check_eq "$(value_of "$capture" CARGO_TARGET_DIR)" "$caller_cargo" "$script cold caller cargo"
    check_eq "$(value_of "$capture" XDG_CACHE_HOME)" "$caller_xdg" "$script cold caller XDG"
    check_eq "$(value_of "$capture" GOCACHE)" "$caller_go" "$script cold caller Go cache"
done
check_exists "$caller_cargo/caller-marker" "cold cleanup deleted caller Cargo target"
check_exists "$caller_xdg/caller-marker" "cold cleanup deleted caller XDG cache"
check_exists "$caller_go/caller-marker" "cold cleanup deleted caller Go cache"

wait_for_file() {
    local path="$1" message="$2"
    local attempt=0
    while (( attempt < 250 )); do
        [[ -e "$path" ]] && return 0
        sleep 0.02
        attempt=$((attempt + 1))
    done
    fail "$message"
    return 1
}

echo "[test] the warm-root lock serializes complete walkthrough and gauntlet processes"
reset_demo_env
export SHATTER_DEMO_CACHE="$SCRATCH/shared warm lock root"
first_capture="$SCRATCH/lock-first.env"
second_capture="$SCRATCH/lock-second.env"
first_reached="$SCRATCH/lock-first.reached"
second_reached="$SCRATCH/lock-second.reached"
release="$SCRATCH/lock.release"
CHECKPOINT_CAPTURE="$first_capture" CHECKPOINT_TOUCH=0 \
CHECKPOINT_REACHED="$first_reached" CHECKPOINT_RELEASE="$release" \
    bash "$REPO_ROOT/demo/walkthrough.sh" > "$first_capture.stdout" 2> "$first_capture.stderr" &
first_pid=$!
wait_for_file "$first_reached" "first warm process did not reach checkpoint" || true
CHECKPOINT_CAPTURE="$second_capture" CHECKPOINT_TOUCH=0 \
CHECKPOINT_REACHED="$second_reached" CHECKPOINT_RELEASE="$release" \
    bash "$REPO_ROOT/demo/gauntlet.sh" > "$second_capture.stdout" 2> "$second_capture.stderr" &
second_pid=$!
sleep 0.2
[[ ! -e "$second_reached" ]] || fail "second warm process entered while first still held the full-process cache lock"
touch "$release"
wait "$first_pid" || true
wait "$second_pid" || true
check_exists "$second_reached" "second warm process did not proceed after lock release"

run_parse_case() {
    local script="$1" expected_status="$2" label="$3"
    shift 3
    local capture="$SCRATCH/parse-$script-$label.env"
    local stdout="$SCRATCH/parse-$script-$label.stdout"
    local stderr="$SCRATCH/parse-$script-$label.stderr"
    local status
    : > "$MKTEMP_LOG"
    rm -f "$capture"
    export CHECKPOINT_CAPTURE="$capture" CHECKPOINT_TOUCH=0
    set +e
    bash "$REPO_ROOT/demo/$script.sh" "$@" > "$stdout" 2> "$stderr"
    status=$?
    set -e
    if [[ "$expected_status" == zero ]]; then
        [[ $status -eq 0 ]] || fail "$script $label should exit zero (got $status)"
    else
        [[ $status -ne 0 ]] || fail "$script $label should reject invalid arguments"
    fi
    [[ ! -s "$MKTEMP_LOG" ]] || fail "$script $label allocated temporary state before returning"
    [[ ! -e "$capture" ]] || fail "$script $label reached checkout before returning"
}

echo "[test] help and argument errors allocate nothing"
reset_demo_env
for script in walkthrough gauntlet; do
    run_parse_case "$script" zero help --help
    run_parse_case "$script" nonzero unknown --definitely-unknown
    run_parse_case "$script" nonzero missing-delay --delay
done

if [[ $FAILURES -ne 0 ]]; then
    echo "[FAIL] demo cache mode regressions: $FAILURES assertion(s) failed" >&2
    exit 1
fi

echo "[ok] warm and cold demo cache ownership, cleanup, locking, and argument lifecycle"
