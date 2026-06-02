#!/usr/bin/env bash
# Frontend that exits immediately after the first successful handshake, then
# behaves like a tiny no-op frontend on replacement spawns. Used to exercise
# scan worker-pool recovery when an idle frontend dies between checkout and use.

set -euo pipefail

PROTOCOL_VERSION="0.1.0"
FLAG_FILE="${SHATTER_DEAD_AFTER_HANDSHAKE_FLAG:-/tmp/shatter-dead-after-handshake-$$}"
SHOULD_EXIT=false
if [ ! -f "$FLAG_FILE" ]; then
  touch "$FLAG_FILE"
  SHOULD_EXIT=true
fi

log() { echo "[dead-after-handshake-once] $*" >&2; }

while IFS= read -r line; do
  [ -z "$line" ] && continue

  id=$(echo "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  command=$(echo "$line" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')

  case "$command" in
    handshake)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"handshake\",\"frontend_version\":\"$PROTOCOL_VERSION\",\"language\":\"dead-once\",\"capabilities\":[\"analyze\",\"execute\",\"instrument\"]}"
      echo "$response"
      log "Sent handshake (should_exit=$SHOULD_EXIT)"
      if [ "$SHOULD_EXIT" = true ]; then
        exit 0
      fi
      ;;
    analyze)
      echo "{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"analyze\",\"functions\":[{\"name\":\"stub\",\"exported\":true,\"params\":[],\"branches\":[],\"dependencies\":[],\"return_type\":{\"kind\":\"unknown\"},\"start_line\":1,\"end_line\":1}]}"
      ;;
    instrument)
      echo "{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"instrument\",\"instrumented\":true,\"output_file\":null}"
      ;;
    execute)
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
