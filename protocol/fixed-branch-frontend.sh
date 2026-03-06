#!/usr/bin/env bash
# Test frontend that always returns the same single-branch path.
#
# Every execute returns branch_path = [{branch_id:0, taken:true, constraint: x > 0}]
# regardless of input. This creates a scenario where triage can predict Skip
# for redundant inputs because all executions produce the same path.

set -euo pipefail

PROTOCOL_VERSION="0.1.0"

log() {
  echo "[fixed-branch] $*" >&2
}

log "Starting fixed-branch test frontend"

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
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"analyze\",\"functions\":[{\"name\":\"f\",\"params\":[{\"name\":\"x\",\"type\":{\"kind\":\"int\"}}],\"branches\":[{\"id\":0,\"line\":1,\"condition_text\":\"x > 0\",\"condition\":{\"kind\":\"bin_op\",\"op\":\"gt\",\"left\":{\"kind\":\"param\",\"name\":\"x\",\"path\":[]},\"right\":{\"kind\":\"const\",\"type\":\"int\",\"value\":0}},\"branch_type\":\"if\"}],\"dependencies\":[],\"return_type\":{\"kind\":\"int\"},\"start_line\":1,\"end_line\":3}]}"
      ;;

    instrument)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"instrument\",\"instrumented\":true,\"output_file\":null}"
      ;;

    execute)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"execute\",\"return_value\":1,\"thrown_error\":null,\"branch_path\":[{\"branch_id\":0,\"line\":1,\"taken\":true,\"constraint\":{\"kind\":\"expr\",\"expr\":{\"kind\":\"bin_op\",\"op\":\"gt\",\"left\":{\"kind\":\"param\",\"name\":\"x\",\"path\":[]},\"right\":{\"kind\":\"const\",\"type\":\"int\",\"value\":0}}}}],\"lines_executed\":[1,2],\"calls_to_external\":[],\"path_constraints\":[],\"side_effects\":[],\"performance\":{\"wall_time_ms\":0.1,\"cpu_time_us\":100,\"heap_used_bytes\":256,\"heap_allocated_bytes\":512}}"
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
