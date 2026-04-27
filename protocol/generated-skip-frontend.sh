#!/usr/bin/env bash
set -euo pipefail

PROTOCOL_VERSION="0.1.0"

while IFS= read -r line; do
  [ -z "$line" ] && continue

  id=$(echo "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  command=$(echo "$line" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')

  case "$command" in
    handshake)
      response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"handshake","frontend_version":"$PROTOCOL_VERSION","language":"test","capabilities":["analyze"]}
EOF
)
      ;;
    analyze)
      file=$(echo "$line" | sed -n 's/.*"file":"\([^"]*\)".*/\1/p')
      if [[ "$file" == *"/generated/"* ]]; then
        response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"error","code":"not_supported","message":"generated files are skipped by default","details":null}
EOF
)
      else
        response=$(cat <<EOF
{"protocol_version":"$PROTOCOL_VERSION","id":$id,"status":"analyze","functions":[{"name":"stub","exported":true,"params":[],"branches":[],"dependencies":[],"return_type":{"kind":"unknown"},"start_line":1,"end_line":1}]}
EOF
)
      fi
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
