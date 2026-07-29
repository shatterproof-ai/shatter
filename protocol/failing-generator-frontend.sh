#!/usr/bin/env bash
# Test frontend that models a config-bound custom generator dying under load
# (str-mt78j).
#
# Every `generate` fails, so the extractor parameter it feeds is left to
# built-in generation; every `execute` then rejects the target with a
# `not_supported` extractor error — the exact degradation observed when a
# generator's database pool times out during a parallel scan.
#
# Usage: bash protocol/failing-generator-frontend.sh

set -euo pipefail

PROTOCOL_VERSION="0.1.0"

while IFS= read -r line; do
  [ -z "$line" ] && continue

  id=$(echo "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  command=$(echo "$line" | sed -n 's/.*"command":"\([^"]*\)".*/\1/p')

  case "$command" in
    handshake)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"handshake\",\"frontend_version\":\"$PROTOCOL_VERSION\",\"language\":\"failing-generator\",\"capabilities\":[\"analyze\",\"execute\",\"instrument\",\"generate\"]}"
      ;;

    instrument)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"instrument\",\"instrumented\":true,\"output_file\":null}"
      ;;

    generate)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"error\",\"code\":\"internal_error\",\"message\":\"PoolTimedOut: timed out acquiring a database connection\",\"details\":null}"
      ;;

    execute)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"error\",\"code\":\"not_supported\",\"message\":\"axum handler has unsupported extractor types: CurrentAccount\",\"details\":null}"
      ;;

    shutdown)
      echo "{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"shutdown_ack\"}"
      exit 0
      ;;

    *)
      response="{\"protocol_version\":\"$PROTOCOL_VERSION\",\"id\":$id,\"status\":\"error\",\"code\":\"invalid_request\",\"message\":\"Unknown command: $command\",\"details\":null}"
      ;;
  esac

  echo "$response"
done
