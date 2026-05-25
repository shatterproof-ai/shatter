#!/usr/bin/env bash
# Frontend that is slow on the first non-handshake request, then fast.
#
# Used to test that after a timeout, the next request drains the stale
# response and maintains correct request/response ID pairing.
#
# Usage: bash protocol/slow-once-frontend.sh

set -euo pipefail

PROTOCOL_VERSION="0.1.0"
SLOW_DONE=0

log() {
  echo "[slow-once] $*" >&2
}

log "Starting slow-once frontend"

while IFS= read -r line; do
  [ -z "$line" ] && continue

  id=$(echo "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  command=$(echo "$line" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')

  case "$command" in
    handshake)
      echo "{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"handshake\",\"frontend_version\":\"$PROTOCOL_VERSION\",\"language\":\"slow-once\",\"capabilities\":[\"analyze\",\"execute\",\"instrument\"]}"
      ;;
    analyze)
      if [ "$SLOW_DONE" -eq 0 ]; then
        SLOW_DONE=1
        log "Sleeping on first analyze (id=$id) to trigger timeout"
        sleep 2
      fi
      echo "{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"analyze\",\"functions\":[{\"name\":\"stub\",\"exported\":true,\"params\":[],\"branches\":[],\"dependencies\":[],\"return_type\":{\"kind\":\"unknown\"},\"start_line\":1,\"end_line\":1}]}"
      ;;
    shutdown)
      echo "{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"shutdown_ack\"}"
      log "Shutting down"
      exit 0
      ;;
    *)
      echo "{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"error\",\"code\":\"invalid_request\",\"message\":\"Unknown command: $command\",\"details\":null}"
      ;;
  esac
done

log "Stdin closed, exiting"
