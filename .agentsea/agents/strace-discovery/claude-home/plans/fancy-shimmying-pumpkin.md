# Plan: str-fpgb.1 — Protocol Registry Scaffold

## Context

The Shatter protocol is defined independently in four places (shatter-core/src/protocol.rs, shatter-ts/src/protocol.ts, shatter-go/protocol/, shatter-rust/src/protocol.rs). There's no single source of truth listing all commands, statuses, capabilities, error codes, and compatibility policy. The `protocol/` directory already has JSON schemas for message types but lacks a high-level registry. This task adds a YAML-based registry that serves as the canonical contract.

Notable discrepancy found: Go frontend defines `not_supported` error code not present in core. Go advertises `teardown` as a capability; TS does not. Core has `select` branch type (Go-specific); TS doesn't.

## Approach

### 1. Create `protocol/registry.yaml`

Single YAML file containing:

- **version**: protocol version ("0.1.0") and compatibility policy
- **commands**: all 8 commands with their request/response field summaries and required capabilities
- **statuses**: all 9 response statuses
- **error_codes**: all 10 (+ `not_supported` from Go = 11) with descriptions and categories
- **capabilities**: base command capabilities + complex_type capabilities
- **setup_levels**: the 4 lifecycle levels
- **generator_kinds**: type_name, param_name
- **branch_types**: all branch types including language-specific ones
- **frontends**: per-frontend metadata (version, language, supported capabilities, timeouts)

### 2. Create `protocol/README.md`

Short doc declaring the registry as the authoritative contract source, explaining how it should be kept in sync, and pointing to the validation script.

### 3. Create `scripts/validate-protocol-registry.py`

Python validation script that:
- Parses registry.yaml
- Greps Rust core protocol.rs for command variants, error codes, status variants
- Greps TS protocol.ts for the same
- Greps Go constants.go/types.go/handler.go for the same
- Reports any commands/statuses/error_codes present in code but missing from registry (or vice versa)
- Exits non-zero on mismatch

Python chosen because: cross-platform, already available, good at YAML parsing + regex, no build step needed.

### 4. No code generation yet

The acceptance criteria say "generated or checked artifacts derive lists from it". The validation script satisfies "checked artifacts" — it verifies the code matches the registry. Code generation from the registry is a future step (would require significant refactoring of how each frontend defines types).

## Files to Create/Modify

| File | Action |
|------|--------|
| `protocol/registry.yaml` | **Create** — the registry |
| `protocol/README.md` | **Create** — docs declaring it authoritative |
| `scripts/validate-protocol-registry.py` | **Create** — validation script |

## Verification

1. Run `python3 scripts/validate-protocol-registry.py` — should pass with 0 exit code
2. Manually verify registry covers all commands/statuses/error codes found in exploration
3. Standard tier: `cargo clippy -- -D warnings` (no Rust changes, but sanity check)
