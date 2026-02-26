#!/usr/bin/env bash
set -euo pipefail

echo "==> Configuring bindgen environment..."
./scripts/configure-bindgen.sh

echo "==> Installing Rust dependencies..."
cargo fetch

echo "==> Installing TypeScript dependencies..."
(cd shatter-ts && npm ci)

echo "==> Installing Go dependencies..."
(cd shatter-go && go mod download)

echo "==> Configuring Claude Code local settings..."
if [ ! -f .claude/settings.local.json ]; then
  mkdir -p .claude
  cat > .claude/settings.local.json << 'SETTINGS'
{
  "permissions": {
    "deny": [
      "Bash(git commit *)",
      "Bash(curl *)",
      "Bash(wget *)"
    ],
    "allow": [
      "Bash(*)",
      "Read",
      "Edit",
      "Write",
      "Glob",
      "Grep"
    ]
  }
}
SETTINGS
  echo "  Created .claude/settings.local.json (devcontainer defaults)"
else
  echo "  .claude/settings.local.json already exists, skipping"
fi

echo "==> Initializing beads issue tracker..."
if [ -f .beads/issues.jsonl ] && [ ! -d .beads/dolt/beads_str ]; then
  bd init --prefix str --from-jsonl --quiet
  echo "  Bootstrapped Dolt database from issues.jsonl"
else
  echo "  Beads database already exists or no JSONL found, skipping"
fi
bd config set beads.role maintainer 2>/dev/null || true

echo "==> Running quick validation..."
cargo check
echo "✓ Devcontainer ready"
