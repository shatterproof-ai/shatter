#!/usr/bin/env bash
# No-op frontend variant that records command ownership by process ID.

set -euo pipefail

PROTOCOL_VERSION="0.1.0"
FRONTEND_LANGUAGE="observer-recording"
LOG_PATH="${SHATTER_OBSERVER_LOG:-}"
EXEC_SLEEP="${SHATTER_OBSERVER_EXEC_SLEEP:-0}"

record() {
  if [[ -n "$LOG_PATH" ]]; then
    printf '%s:%s\n' "$1" "$$" >> "$LOG_PATH"
  fi
}

while IFS= read -r line; do
  [[ -z "$line" ]] && continue

  id=$(echo "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  command=$(echo "$line" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')
  record "$command"

  case "$command" in
    handshake)
      response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"handshake","frontend_version":"$PROTOCOL_VERSION","language":"$FRONTEND_LANGUAGE","capabilities":["analyze","execute","instrument","setup","teardown","generate"]}
EOF
)
      ;;
    analyze)
      response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"analyze","functions":[{"name":"stub","exported":true,"params":[],"branches":[],"dependencies":[],"return_type":{"kind":"unknown"},"start_line":1,"end_line":1}]}
EOF
)
      ;;
    instrument)
      response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"instrument","instrumented":true,"output_file":null}
EOF
)
      ;;
    execute)
      sleep "$EXEC_SLEEP"
      response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"execute","return_value":null,"thrown_error":null,"branch_path":[],"lines_executed":[],"calls_to_external":[],"path_constraints":[],"side_effects":[],"performance":{"wall_time_ms":0.0,"cpu_time_us":0,"heap_used_bytes":0,"heap_allocated_bytes":0}}
EOF
)
      ;;
    setup)
      response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"setup","setup_context":{"observer":true}}
EOF
)
      ;;
    teardown)
      response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"teardown_ack"}
EOF
)
      ;;
    generate)
      response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"generate","value":42}
EOF
)
      ;;
    shutdown)
      response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"shutdown_ack"}
EOF
)
      echo "$response"
      exit 0
      ;;
    *)
      response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"error","code":"invalid_request","message":"Unknown command: $command","details":null}
EOF
)
      ;;
  esac

  echo "$response"
done
