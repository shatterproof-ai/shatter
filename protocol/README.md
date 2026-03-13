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

## Changing the protocol

See [GOVERNANCE.md](GOVERNANCE.md) for the mandatory process when making any protocol change — including required updates to schemas, fixtures, conformance cases, and all frontend implementations.

Quick registry sync check:

```bash
python3 scripts/validate-protocol-registry.py
```

## Validation

```bash
python3 scripts/validate-protocol-registry.py
```

Exits 0 on success, 1 on mismatch. Intended for CI integration.
