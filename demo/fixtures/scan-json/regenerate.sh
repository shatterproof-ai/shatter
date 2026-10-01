#!/usr/bin/env bash
# Regenerates the real-CLI scan JSON fixtures used by
# demo/test_gauntlet_check_output.py (str-49drv.149):
#   failed.json       one failed function (`spin` times out) + one completed
#   interrupted.json  only interrupted functions (total budget exhausted)
# Usage: demo/fixtures/scan-json/regenerate.sh [path/to/shatter]
# Absolute checkout paths are rewritten to /fixture so the files are stable.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
shatter="${1:-shatter}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cp -r "$here/src" "$work/src"
cd "$work"
export SHATTER_ALLOW_HOST_WRITES=1
"$shatter" scan --timeout-per-fn 5 --parallelism 1 --format json -o failed.json src >/dev/null
"$shatter" scan --timeout-total 1 --parallelism 1 --format json -o interrupted.json src >/dev/null
for f in failed interrupted; do
    sed "s#$work#/fixture#g" "$f.json" >"$here/$f.json"
done
