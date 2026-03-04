#!/usr/bin/env bash
# Slow frontend for testing FrontendError::Timeout.
#
# Sleeps before responding to any command, ensuring the core's request
# timeout fires first. Used by integration tests in frontend.rs.
#
# Usage: bash protocol/slow-frontend.sh

set -euo pipefail

# Sleep long enough that any reasonable test timeout expires first.
while IFS= read -r line; do
  sleep 5
done
