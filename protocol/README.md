# Protocol Registry

`registry.yaml` is the **authoritative source of truth** for the Shatter JSON-over-stdio protocol contract between `shatter-core` and all language frontends.

## What it covers

- **Commands** — every message core may send to a frontend
- **Response statuses** — every status a frontend may return
- **Error codes** — canonical error code strings and their categories
- **Capabilities** — command and complex-type capabilities advertised during handshake
- **Setup levels**, **generator kinds**, **branch types** — protocol enumerations
- **Frontends** — per-frontend metadata (supported capabilities, timeouts)
- **Compatibility policy** — versioning and mismatch behavior

## Keeping it in sync

When adding a new command, status, error code, or capability:

1. Update `registry.yaml` first
2. Implement in the relevant codebase(s)
3. Run `python3 scripts/validate-protocol-registry.py` to verify consistency

The validation script checks that every command, status, and error code in the source files has a corresponding entry in the registry (and vice versa).

## Validation

```bash
python3 scripts/validate-protocol-registry.py
```

Exits 0 on success, 1 on mismatch. Intended for CI integration.
