#!/usr/bin/env bash
# Test frontend that always reports the same opaque branch path.
#
# The first two executions return immediately so the concolic loop can discover
# a path and hit plateau. Later executions sleep briefly, allowing tests to
# verify that plateau fuzzing stops on timeout_explore instead of exhausting its
# own fuzz-phase budget.

set -euo pipefail

PROTOCOL_VERSION="0.1.0"
execute_count=0

log() {
  echo "[unknown-branch-fuzz] $*" >&2
}

log "Starting unknown-branch fuzz test frontend"

while IFS= read -r line; do
  [ -z "$line" ] && continue

  log "Received: $line"

  id=$(echo "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  command=$(echo "$line" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')

  case "$command" in
    handshake)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"handshake\",\"frontend_version\":\"$PROTOCOL_VERSION\",\"language\":\"test\",\"capabilities\":[\"analyze\",\"execute\",\"instrument\"]}"
      ;;

    analyze)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"analyze\",\"functions\":[{\"name\":\"f\",\"params\":[{\"name\":\"x\",\"type\":{\"kind\":\"int\"}}],\"branches\":[{\"id\":0,\"line\":1,\"condition_text\":\"opaque(x)\",\"condition\":{\"kind\":\"unknown\"},\"branch_type\":\"if\"}],\"dependencies\":[],\"return_type\":{\"kind\":\"int\"},\"start_line\":1,\"end_line\":3}]}"
      ;;

    instrument)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"instrument\",\"instrumented\":true,\"output_file\":null}"
      ;;

    execute)
      execute_count=$((execute_count + 1))
      if [ "$execute_count" -gt 2 ]; then
        sleep 0.01
      fi
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"execute\",\"return_value\":1,\"thrown_error\":null,\"branch_path\":[{\"branch_id\":0,\"line\":1,\"taken\":true,\"constraint\":{\"kind\":\"unknown\",\"hint\":\"opaque\"}}],\"lines_executed\":[1,2],\"calls_to_external\":[],\"path_constraints\":[],\"side_effects\":[],\"performance\":{\"wall_time_ms\":0.1,\"cpu_time_us\":100,\"heap_used_bytes\":256,\"heap_allocated_bytes\":512}}"
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
