# str-fpgb.12: Rust handshake capability parity

## Context
The Rust frontend (`shatter-rust`) handles `setup` and `teardown` commands but doesn't advertise them in the handshake capabilities list, making the negotiated contract inaccurate.

## Changes

### File: `shatter-rust/src/handler.rs`

1. **Add failing test** — In `handshake_returns_all_capabilities` (line 638), add assertions for `"setup"` and `"teardown"`. Verify test fails.

2. **Fix handshake** — Add `"setup"` and `"teardown"` to the capabilities vec at line 208-213.

3. **Verify** — `cargo test -p shatter-rust` + `cargo clippy -p shatter-rust -- -D warnings`

## Verification
- `cargo test -p shatter-rust` — all tests pass
- `cargo clippy -p shatter-rust -- -D warnings` — clean
- No E2E/walkthrough needed (handshake-only change, no pipeline impact)
