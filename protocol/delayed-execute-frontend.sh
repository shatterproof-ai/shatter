#!/usr/bin/env bash
# Frontend that delays the first Execute response.
#
# Responds to handshake, analyze, and instrument immediately but sleeps
# before the first execute response, causing a per-request timeout. The
# second execute response is immediate, exercising the drain-after-timeout
# code path in Frontend::send().
#
# Usage: bash protocol/delayed-execute-frontend.sh
#
# Set DELAY_SECONDS to control the sleep duration (default: 5).

set -euo pipefail

PROTOCOL_VERSION="0.1.0"
FRONTEND_LANGUAGE="delayed"
DELAY=${DELAY_SECONDS:-5}
execute_count=0

log() {
  echo "[delayed-frontend] $*" >&2
}

log "Starting delayed-execute frontend (protocol $PROTOCOL_VERSION, delay=${DELAY}s)"

while IFS= read -r line; do
  [ -z "$line" ] && continue

  log "Received: $line"

  id=$(echo "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  command=$(echo "$line" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')

  case "$command" in
    handshake)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"handshake\",\"frontend_version\":\"$PROTOCOL_VERSION\",\"language\":\"$FRONTEND_LANGUAGE\",\"capabilities\":[\"analyze\",\"execute\",\"instrument\"]}"
      ;;

    analyze)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"analyze\",\"functions\":[{\"name\":\"stub\",\"exported\":true,\"params\":[],\"branches\":[],\"dependencies\":[],\"return_type\":{\"kind\":\"unknown\"},\"start_line\":1,\"end_line\":1}]}"
      ;;

    instrument)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"instrument\",\"instrumented\":true,\"output_file\":null}"
      ;;

    execute)
      execute_count=$((execute_count + 1))
      if [ "$execute_count" -eq 1 ]; then
        log "Delaying first execute response by ${DELAY}s"
        sleep "$DELAY"
      fi
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
