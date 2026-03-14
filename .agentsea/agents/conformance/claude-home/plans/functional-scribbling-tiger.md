# Plan: str-28vd.7 — Rust dependency tool rollout

## Context

The Shatter project has shell script infrastructure (`scripts/quality/check-rust.sh`) that already supports `--deny`, `--nextest`, and `--udeps` flags, plus CI documentation (`docs/CI-INTEGRATION.md`) describing how these tools fit. What's missing is the actual `deny.toml` configuration file and any docs updates to reflect that the tooling is now configured (not just planned).

## Steps

### 1. Create `deny.toml` at repo root
- Configure cargo-deny with sensible defaults for the Shatter workspace
- **Licenses**: allow permissive licenses (MIT, Apache-2.0, BSD-2/3, ISC, Unicode-3.0, Zlib, etc.); deny copyleft (GPL, AGPL) unless explicitly excepted
- **Bans**: no banned crates initially; enable duplicate detection as warnings
- **Advisories**: deny known vulnerabilities, warn on unmaintained/notice advisories; use bundled advisory DB
- **Sources**: allow crates.io only; deny unknown registries/git sources

### 2. Verify `check-rust.sh` works with deny.toml
- Run `cargo deny check` (if installed) to confirm the config is valid
- Fix any issues with current dependencies

### 3. Update `docs/CI-INTEGRATION.md`
- Change references from "not yet configured" to "configured" for cargo-deny
- Update the tooling table to reflect deny.toml is checked in
- Keep cargo-nextest and cargo-udeps as "optional, used when installed" (no config needed for them)

### 4. Run quality checks
- `./scripts/quality/check-rust.sh --deny` to verify integration
- Standard tier: `cargo test && cargo clippy -- -D warnings`

## Files to modify
- **Create**: `deny.toml` (repo root)
- **Edit**: `docs/CI-INTEGRATION.md` (update status of cargo-deny)

## Files to read (already explored)
- `scripts/quality/check-rust.sh` — already has `--deny` flag support
- `scripts/quality/check-tooling.sh` — already detects cargo-deny
- `scripts/quality/check-all.sh` — already passes `--deny` in strict mode

## Verification
1. `cargo deny check` passes with the new config
2. `./scripts/quality/check-rust.sh --deny` runs successfully
3. `cargo test && cargo clippy -- -D warnings` still passes
