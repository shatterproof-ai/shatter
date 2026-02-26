#!/usr/bin/env bash
# Test frontend for concolic exploration integration tests.
#
# Simulates a function f(x) with three branches:
#   if (x > 10)       → branch 0
#     if (x == 42)     → branch 1
#       return "found"
#     return "big"
#   return "small"
#
# The frontend inspects the "inputs" array from execute commands and returns
# branch decisions with symbolic constraints so the orchestrator can use Z3
# to discover all three paths.

set -euo pipefail

PROTOCOL_VERSION="0.1.0"

log() {
  echo "[concolic-test] $*" >&2
}

log "Starting concolic test frontend"

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
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"analyze\",\"functions\":[{\"name\":\"f\",\"params\":[{\"name\":\"x\",\"type\":{\"kind\":\"int\"}}],\"branches\":[{\"id\":0,\"line\":1,\"condition_text\":\"x > 10\",\"condition\":{\"kind\":\"bin_op\",\"op\":\"gt\",\"left\":{\"kind\":\"param\",\"name\":\"x\",\"path\":[]},\"right\":{\"kind\":\"const\",\"type\":\"int\",\"value\":10}},\"branch_type\":\"if\"},{\"id\":1,\"line\":2,\"condition_text\":\"x == 42\",\"condition\":{\"kind\":\"bin_op\",\"op\":\"eq\",\"left\":{\"kind\":\"param\",\"name\":\"x\",\"path\":[]},\"right\":{\"kind\":\"const\",\"type\":\"int\",\"value\":42}},\"branch_type\":\"if\"}],\"dependencies\":[],\"return_type\":{\"kind\":\"str\"},\"start_line\":1,\"end_line\":5}]}"
      ;;

    instrument)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"instrument\",\"instrumented\":true,\"output_file\":null}"
      ;;

    execute)
      # Extract x value from inputs array. Expected format: "inputs":[N]
      x=$(echo "$line" | sed -n 's/.*"inputs":\[\([0-9\-]*\).*/\1/p')
      if [ -z "$x" ]; then
        x=0
      fi

      log "Executing f($x)"

      if [ "$x" -gt 10 ] 2>/dev/null; then
        branch0_taken=true
        if [ "$x" -eq 42 ] 2>/dev/null; then
          branch1_taken=true
          ret_val="\"found\""
        else
          branch1_taken=false
          ret_val="\"big\""
        fi
        # When x > 10: branch 0 taken=true, branch 1 depends on x==42
        # Constraint for branch 0 (x > 10) is the condition itself.
        # When taken=true, the path includes the condition as-is.
        # When taken=false, the orchestrator records the condition and negating it produces the opposite.
        response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"execute\",\"return_value\":$ret_val,\"thrown_error\":null,\"branch_path\":[{\"branch_id\":0,\"line\":1,\"taken\":true,\"constraint\":{\"kind\":\"expr\",\"expr\":{\"kind\":\"bin_op\",\"op\":\"gt\",\"left\":{\"kind\":\"param\",\"name\":\"x\",\"path\":[]},\"right\":{\"kind\":\"const\",\"type\":\"int\",\"value\":10}}}},{\"branch_id\":1,\"line\":2,\"taken\":$branch1_taken,\"constraint\":{\"kind\":\"expr\",\"expr\":{\"kind\":\"bin_op\",\"op\":\"eq\",\"left\":{\"kind\":\"param\",\"name\":\"x\",\"path\":[]},\"right\":{\"kind\":\"const\",\"type\":\"int\",\"value\":42}}}}],\"lines_executed\":[1,2,3],\"calls_to_external\":[],\"path_constraints\":[],\"side_effects\":[],\"performance\":{\"wall_time_ms\":0.1,\"cpu_time_us\":100,\"heap_used_bytes\":512,\"heap_allocated_bytes\":1024}}"
      else
        # x <= 10: branch 0 taken=false, branch 1 not reached
        response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"execute\",\"return_value\":\"small\",\"thrown_error\":null,\"branch_path\":[{\"branch_id\":0,\"line\":1,\"taken\":false,\"constraint\":{\"kind\":\"expr\",\"expr\":{\"kind\":\"bin_op\",\"op\":\"gt\",\"left\":{\"kind\":\"param\",\"name\":\"x\",\"path\":[]},\"right\":{\"kind\":\"const\",\"type\":\"int\",\"value\":10}}}}],\"lines_executed\":[1,4],\"calls_to_external\":[],\"path_constraints\":[],\"side_effects\":[],\"performance\":{\"wall_time_ms\":0.05,\"cpu_time_us\":50,\"heap_used_bytes\":256,\"heap_allocated_bytes\":512}}"
      fi
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
