#!/usr/bin/env bash
# No-op reference frontend for the Shatter protocol.
#
# Responds to all commands with valid stub responses. Useful for:
# - Testing the core engine's protocol handling
# - Understanding the protocol message format
# - Bootstrapping new language frontend implementations
#
# Usage: bash protocol/noop-frontend.sh
#
# The frontend reads newline-delimited JSON from stdin and writes
# newline-delimited JSON responses to stdout. Debug output goes to stderr.

set -euo pipefail

PROTOCOL_VERSION="0.1.0"
FRONTEND_LANGUAGE="noop"

log() {
  echo "[noop-frontend] $*" >&2
}

log "Starting no-op frontend (protocol $PROTOCOL_VERSION)"

while IFS= read -r line; do
  # Skip empty lines
  [ -z "$line" ] && continue

  log "Received: $line"

  # Extract fields using lightweight JSON parsing.
  # For a production frontend, use a proper JSON parser.
  id=$(echo "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  command=$(echo "$line" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')

  case "$command" in
    handshake)
      response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"handshake","frontend_version":"$PROTOCOL_VERSION","language":"$FRONTEND_LANGUAGE","capabilities":["analyze","execute","instrument"]}
EOF
)
      ;;

    analyze)
      file=$(echo "$line" | sed -n 's/.*"file":"\([^"]*\)".*/\1/p')
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
      response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"execute","return_value":null,"thrown_error":null,"branch_path":[],"lines_executed":[],"calls_to_external":[],"path_constraints":[],"side_effects":[],"performance":{"wall_time_ms":0.0,"cpu_time_us":0,"heap_used_bytes":0,"heap_allocated_bytes":0}}
EOF
)
      ;;

    shutdown)
      response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"shutdown_ack"}
EOF
)
      echo "$response"
      log "Sent: $response"
      log "Shutting down"
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
  log "Sent: $response"
done

log "Stdin closed, exiting"
