#!/usr/bin/env bash
# gate-wrapper.sh <label> <cmd...>
#
# Machine-wide governance for heavyweight gates (str-35vtk.5):
#  - counting semaphore of SHATTER_HEAVY_SLOTS flock slots (default
#    max(1, nproc/8)) under ${XDG_RUNTIME_DIR:-/tmp}/shatter-heavy-slots/,
#    shared by every worktree on the machine;
#  - nice/ionice so gates yield to interactive work;
#  - timing CSV appended to ~/.cache/shatter/gate-times.csv:
#    timestamp,worktree,label,wall_seconds,exit_code,loadavg_1min,slot,wait_seconds
#
# Re-entrancy: nested wrapped tasks (check -> conformance) pass through via
# SHATTER_GATE_LOCK_HELD so composition cannot deadlock. If the semaphore
# cannot operate (no flock, unusable lock dir), the gate still runs —
# governance degrades to a warning, never to a refusal.
set -u

label="${1:?usage: gate-wrapper.sh <label> <cmd...>}"
shift

if [ "${SHATTER_GATE_LOCK_HELD:-}" = "1" ]; then
  exec "$@"
fi

run_governed() {
  # $1 = slot label for the CSV ("-" when ungoverned), $2 = wait seconds,
  # rest = the gate command
  local slot="$1" waited="$2" start end rc load csv tmp
  shift 2
  start=$(date +%s)
  load=$(cut -d' ' -f1 /proc/loadavg 2>/dev/null || echo 0)
  export SHATTER_GATE_LOCK_HELD=1
  if command -v ionice >/dev/null 2>&1; then
    nice -n 10 ionice -c2 -n7 "$@" &
  else
    nice -n 10 "$@" &
  fi
  local child=$!
  # Forward termination to the gate and still write the CSV row (review I3).
  trap 'kill -TERM "$child" 2>/dev/null' TERM INT
  wait "$child"
  rc=$?
  trap - TERM INT
  end=$(date +%s)

  csv="${HOME}/.cache/shatter/gate-times.csv"
  mkdir -p "$(dirname "$csv")" 2>/dev/null || true
  echo "$(date -Is),$PWD,$label,$((end - start)),$rc,$load,$slot,$waited" >> "$csv" 2>/dev/null || true
  # Trim to a low watermark so the rewrite doesn't happen on every run
  # (review M1); mktemp avoids concurrent-truncation clobber.
  if [ "$(wc -l < "$csv" 2>/dev/null || echo 0)" -gt 10000 ]; then
    tmp=$(mktemp "$csv.XXXXXX" 2>/dev/null) || tmp=""
    if [ -n "$tmp" ]; then
      tail -n 8000 "$csv" > "$tmp" && mv "$tmp" "$csv"
    fi
  fi
  return "$rc"
}

# Degrade to ungoverned execution when the semaphore cannot work (review I2/M3).
if ! command -v flock >/dev/null 2>&1; then
  echo "[gate-wrapper] $label: flock unavailable; running ungoverned" >&2
  run_governed "-" 0 "$@"
  exit $?
fi

slots="${SHATTER_HEAVY_SLOTS:-}"
case "$slots" in
  ''|*[!0-9]*)
    if [ -n "$slots" ]; then
      echo "[gate-wrapper] $label: SHATTER_HEAVY_SLOTS='$slots' is not a number; using default" >&2
    fi
    ncpu=$(nproc 2>/dev/null || echo 8)
    slots=$(( ncpu / 8 )); [ "$slots" -ge 1 ] || slots=1
    ;;
esac

lockdir="${XDG_RUNTIME_DIR:-/tmp}/shatter-heavy-slots"
if ! mkdir -p "$lockdir" 2>/dev/null || [ ! -w "$lockdir" ]; then
  echo "[gate-wrapper] $label: lock dir $lockdir unusable; running ungoverned" >&2
  run_governed "-" 0 "$@"
  exit $?
fi

# Non-blocking sweep over all slots using bash automatic fd allocation
# (review C1: a hand-rolled `eval exec N>` sweep with 2>/dev/null clobbers
# fd 10 via bash's stderr save/restore, permanently disabling slot 2).
try_slots() {
  acquired=""
  local i
  for i in $(seq 1 "$slots"); do
    exec {fd}>"$lockdir/slot-$i" || continue
    if flock -n "$fd"; then
      acquired=$i
      lockfd=$fd
      return 0
    fi
    exec {fd}>&-
  done
  return 1
}

wait_start=$(date +%s)
until try_slots; do
  # Re-sweep every few seconds rather than pinning slot 1 (review I1:
  # blocking on one slot convoys all waiters behind its holder while other
  # slots free up).
  now=$(date +%s)
  if [ $(( (now - wait_start) % 60 )) -lt 3 ] && [ $((now - wait_start)) -ge 3 ]; then
    echo "[gate-wrapper] $label: waiting for a heavyweight slot ($((now - wait_start))s, $slots slots)" >&2
  fi
  sleep 3
done
waited=$(( $(date +%s) - wait_start ))

run_governed "$acquired" "$waited" "$@"
rc=$?
exec {lockfd}>&-
exit "$rc"
