#!/usr/bin/env bash
# WS0 (str-35vtk.1) concurrency headline: N checkouts running `task check`
# simultaneously. WARNING: saturates the machine for 30-60 minutes — run at a
# convenient time. Results append to docs/perf/gate-budgets.md by hand.
#
# Usage: scripts/concurrency-headline.sh <checkout-dir>...
#   e.g. scripts/concurrency-headline.sh ~/project/shatter \
#          ~/.local/share/worktrees/shatter/str-35vtk.1-baseline [...]
# Each checkout should be pre-built (task build) so the measurement is the
# check itself, not cold compiles. Records per-run wall/exit and samples
# 1-minute loadavg every 15s.
set -u
[ $# -ge 2 ] || { echo "need >=2 checkout dirs" >&2; exit 2; }

stamp=$(date +%Y%m%d-%H%M%S)
out="/tmp/shatter-concurrency-headline-$stamp"
mkdir -p "$out"
echo "results -> $out"

( while true; do echo "$(date -Is) $(cut -d' ' -f1-3 /proc/loadavg)"; sleep 15; done ) > "$out/loadavg.log" &
sampler=$!

overall_start=$(date +%s)
pids=()
i=0
for dir in "$@"; do
  i=$((i+1))
  ( cd "$dir" || exit 9
    start=$(date +%s)
    task --force check > "$out/check-$i.log" 2>&1
    rc=$?
    end=$(date +%s)
    echo "checkout=$dir wall=$((end-start))s rc=$rc" > "$out/result-$i.txt"
  ) &
  pids+=($!)
done
for p in "${pids[@]}"; do wait "$p"; done
overall_end=$(date +%s)
kill "$sampler" 2>/dev/null

{
  echo "concurrent checkouts: $#"
  echo "aggregate wall: $((overall_end-overall_start))s"
  cat "$out"/result-*.txt
  echo "peak 1-min loadavg: $(awk '{print $2}' "$out/loadavg.log" | sort -n | tail -1)"
} | tee "$out/summary.txt"
