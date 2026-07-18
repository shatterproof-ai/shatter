#!/usr/bin/env bash
# gate-wrapper.sh <label> <cmd...>
#
# Machine-wide governance for heavyweight gates (str-35vtk.5):
#  - counting semaphore of SHATTER_HEAVY_SLOTS flock slots (default
#    max(1, nproc/8)) under ${XDG_RUNTIME_DIR:-/tmp}/shatter-heavy-slots/,
#    shared by every worktree on the machine;
#  - nice/ionice so gates yield to interactive work;
#  - timing CSV appended to ~/.cache/shatter/gate-times.csv:
#    timestamp,worktree,label,wall_seconds,exit_code,loadavg_1min,slot
#
# Re-entrancy: nested wrapped tasks (check -> conformance) pass through via
# SHATTER_GATE_LOCK_HELD so composition cannot deadlock.
set -u

label="${1:?usage: gate-wrapper.sh <label> <cmd...>}"
shift

if [ "${SHATTER_GATE_LOCK_HELD:-}" = "1" ]; then
  exec "$@"
fi

slots="${SHATTER_HEAVY_SLOTS:-}"
if [ -z "$slots" ]; then
  ncpu=$(nproc 2>/dev/null || echo 8)
  slots=$(( ncpu / 8 )); [ "$slots" -ge 1 ] || slots=1
fi

lockdir="${XDG_RUNTIME_DIR:-/tmp}/shatter-heavy-slots"
mkdir -p "$lockdir" 2>/dev/null || true

# Try each slot non-blocking; if all busy, block on slot 1 with periodic
# progress messages so a queued gate is never a silent hang.
acquired=""
for i in $(seq 1 "$slots"); do
  eval "exec $((8 + i))>\"$lockdir/slot-$i\"" 2>/dev/null || continue
  if flock -n $((8 + i)); then
    acquired=$i
    break
  fi
  eval "exec $((8 + i))>&-"
done
if [ -z "$acquired" ]; then
  echo "[gate-wrapper] $label: all $slots heavyweight slots busy; waiting for slot 1..." >&2
  exec 9>"$lockdir/slot-1"
  while ! flock -w 60 9; do
    echo "[gate-wrapper] $label: still waiting for a heavyweight slot ($(date +%H:%M:%S))" >&2
  done
  acquired=1
fi

export SHATTER_GATE_LOCK_HELD=1

start=$(date +%s)
load=$(cut -d' ' -f1 /proc/loadavg 2>/dev/null || echo 0)
if command -v ionice >/dev/null 2>&1; then
  nice -n 10 ionice -c2 -n7 "$@"
else
  nice -n 10 "$@"
fi
rc=$?
end=$(date +%s)

csv="${HOME}/.cache/shatter/gate-times.csv"
mkdir -p "$(dirname "$csv")" 2>/dev/null || true
echo "$(date -Is),$PWD,$label,$((end - start)),$rc,$load,$acquired" >> "$csv" 2>/dev/null || true
# Keep the log bounded.
if [ "$(wc -l < "$csv" 2>/dev/null || echo 0)" -gt 10000 ]; then
  tail -n 10000 "$csv" > "$csv.tmp" && mv "$csv.tmp" "$csv"
fi

exit "$rc"
