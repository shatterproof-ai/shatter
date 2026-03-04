#!/usr/bin/env bash
# Frontend that responds instantly to all commands except execute, which
# sleeps for SLOW_EXECUTE_SECS (default 60). Used to test the scan
# orchestrator's timeout-respawn behavior: after a per-function timeout
# the tainted frontend must be discarded and replaced so the next
# function doesn't hit an ID mismatch from the stale buffered response.

set -euo pipefail

PROTOCOL_VERSION="0.1.0"
SLOW_EXECUTE_SECS="${SLOW_EXECUTE_SECS:-60}"

log() { echo "[slow-execute-frontend] $*" >&2; }

log "Starting (execute delay=${SLOW_EXECUTE_SECS}s)"

while IFS= read -r line; do
  [ -z "$line" ] && continue
  log "Received: $line"

  id=$(echo "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  command=$(echo "$line" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')

  case "$command" in
    handshake)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"handshake\",\"frontend_version\":\"$PROTOCOL_VERSION\",\"language\":\"slow-execute\",\"capabilities\":[\"analyze\",\"execute\",\"instrument\"]}"
      ;;
    analyze)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"analyze\",\"functions\":[{\"name\":\"stub\",\"exported\":true,\"params\":[],\"branches\":[],\"dependencies\":[],\"return_type\":{\"kind\":\"unknown\"},\"start_line\":1,\"end_line\":1}]}"
      ;;
    instrument)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"instrument\",\"instrumented\":true,\"output_file\":null}"
      ;;
    execute)
      log "Sleeping ${SLOW_EXECUTE_SECS}s to trigger timeout..."
      sleep "$SLOW_EXECUTE_SECS"
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"execute\",\"return_value\":null,\"thrown_error\":null,\"branch_path\":[],\"lines_executed\":[],\"calls_to_external\":[],\"path_constraints\":[],\"side_effects\":[],\"performance\":{\"wall_time_ms\":0.0,\"cpu_time_us\":0,\"heap_used_bytes\":0,\"heap_allocated_bytes\":0}}"
      ;;
    shutdown)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"shutdown_ack\"}"
      echo "$response"
      log "Sent: $response"
      log "Shutting down"
      exit 0
      ;;
    *)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"error\",\"code\":\"invalid_request\",\"message\":\"Unknown command: $command\",\"details\":null}"
      ;;
  esac

  echo "$response"
  log "Sent: $response"
done

log "Stdin closed, exiting"
