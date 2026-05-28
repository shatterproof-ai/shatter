#!/usr/bin/env bash
# Frontend that injects an extra response line after the first execute on
# the FIRST process that runs (using a temp-file flag), simulating a pipe
# misalignment that produces an IdMismatch. Subsequent process spawns (i.e.
# replacement pool workers) behave normally. Used to test the scan
# orchestrator's poisoned-frontend detection (str-quhk).

set -euo pipefail

PROTOCOL_VERSION="0.1.0"
EXECUTE_COUNT=0
# Global flag file: only the first process to create it injects the dup.
FLAG_FILE="${SHATTER_MISMATCH_FLAG:-/tmp/shatter-id-mismatch-injected-$$}"
SHOULD_INJECT=false
if [ ! -f "$FLAG_FILE" ]; then
  touch "$FLAG_FILE"
  SHOULD_INJECT=true
fi

log() { echo "[id-mismatch-frontend] $*" >&2; }

log "Starting (should_inject=$SHOULD_INJECT, flag=$FLAG_FILE)"

while IFS= read -r line; do
  [ -z "$line" ] && continue
  log "Received: $line"

  id=$(echo "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  command=$(echo "$line" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')

  case "$command" in
    handshake)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"handshake\",\"frontend_version\":\"$PROTOCOL_VERSION\",\"language\":\"id-mismatch\",\"capabilities\":[\"analyze\",\"execute\",\"instrument\"]}"
      ;;
    analyze)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"analyze\",\"functions\":[{\"name\":\"stub\",\"exported\":true,\"params\":[],\"branches\":[],\"dependencies\":[],\"return_type\":{\"kind\":\"unknown\"},\"start_line\":1,\"end_line\":1}]}"
      ;;
    instrument)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"instrument\",\"instrumented\":true,\"output_file\":null}"
      ;;
    execute)
      EXECUTE_COUNT=$((EXECUTE_COUNT + 1))
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"execute\",\"return_value\":null,\"thrown_error\":null,\"branch_path\":[],\"lines_executed\":[],\"calls_to_external\":[],\"path_constraints\":[],\"side_effects\":[],\"performance\":{\"wall_time_ms\":0.0,\"cpu_time_us\":0,\"heap_used_bytes\":0,\"heap_allocated_bytes\":0}}"
      echo "$response"
      log "Sent: $response"
      if [ "$SHOULD_INJECT" = true ] && [ "$EXECUTE_COUNT" -eq 1 ]; then
        # Inject a duplicate response — simulates a pipe misalignment.
        log "Injecting duplicate response to cause IdMismatch"
        echo "$response"
      fi
      continue
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
