#!/usr/bin/env bash
# Frontend that exits before responding to the first two Execute requests,
# then behaves like a tiny no-op frontend. Used to exercise the scan
# worker-pool retry budget for transient frontend transport loss.

set -euo pipefail

PROTOCOL_VERSION="0.1.0"
COUNTER_FILE="${SHATTER_EXECUTE_EXIT_COUNTER:?SHATTER_EXECUTE_EXIT_COUNTER must be set}"

log() { echo "[execute-exits-twice] $*" >&2; }

while IFS= read -r line; do
  [ -z "$line" ] && continue

  id=$(echo "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  command=$(echo "$line" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')

  case "$command" in
    handshake)
      echo "{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"handshake\",\"frontend_version\":\"$PROTOCOL_VERSION\",\"language\":\"execute-exits-twice\",\"capabilities\":[\"analyze\",\"execute\",\"instrument\"]}"
      ;;
    analyze)
      echo "{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"analyze\",\"functions\":[{\"name\":\"stub\",\"exported\":true,\"params\":[],\"branches\":[],\"dependencies\":[],\"return_type\":{\"kind\":\"unknown\"},\"start_line\":1,\"end_line\":1}]}"
      ;;
    instrument)
      echo "{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"instrument\",\"instrumented\":true,\"output_file\":null}"
      ;;
    execute)
      count=0
      if [ -f "$COUNTER_FILE" ]; then
        count=$(cat "$COUNTER_FILE")
      fi
      count=$((count + 1))
      printf '%s' "$count" > "$COUNTER_FILE"
      if [ "$count" -le 2 ]; then
        log "exiting before execute response attempt=$count"
        exit 101
      fi
      echo "{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"execute\",\"return_value\":null,\"thrown_error\":null,\"branch_path\":[],\"lines_executed\":[],\"calls_to_external\":[],\"path_constraints\":[],\"side_effects\":[],\"performance\":{\"wall_time_ms\":0.0,\"cpu_time_us\":0,\"heap_used_bytes\":0,\"heap_allocated_bytes\":0}}"
      ;;
    shutdown)
      echo "{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"shutdown_ack\"}"
      exit 0
      ;;
    *)
      echo "{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"error\",\"code\":\"invalid_request\",\"message\":\"Unknown command: $command\",\"details\":null}"
      ;;
  esac
done
