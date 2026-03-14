# Plan: Rust frontend version check parity (str-fpgb.13)

## Context

The Rust frontend (`shatter-rust`) uses exact string equality for protocol version checking (`req.protocol_version != PROTOCOL_VERSION`), while shatter-core, shatter-ts, and shatter-go all use major.minor comparison (ignoring patch). This means if the core sends `"0.1.1"` the Rust frontend rejects it, but TS and Go accept it.

## Changes

### 1. Add `is_version_compatible()` to `shatter-rust/src/handler.rs`

Add a helper function matching the Go/TS pattern:

```rust
/// Check major.minor compatibility, ignoring patch version.
fn is_version_compatible(version: &str) -> bool {
    let req = parse_major_minor(version);
    let ours = parse_major_minor(protocol::PROTOCOL_VERSION);
    match (req, ours) {
        (Some((rmaj, rmin)), Some((omaj, omin))) => rmaj == omaj && rmin == omin,
        _ => false,
    }
}

fn parse_major_minor(version: &str) -> Option<(u32, u32)> {
    let mut parts = version.splitn(3, '.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}
```

### 2. Update `dispatch()` in `handler.rs:177`

Replace:
```rust
if req.protocol_version != protocol::PROTOCOL_VERSION {
```
With:
```rust
if !is_version_compatible(&req.protocol_version) {
```

### 3. Add failing test first (write before fixing)

```rust
#[test]
fn compatible_patch_version_is_accepted() {
    // major.minor match with different patch should succeed (parity with TS/Go)
    let resp = send_recv(
        r#"{"protocol_version":"0.1.99","id":1,"command":"handshake","capabilities":[]}"#,
    );
    assert_eq!(resp.status, "handshake");
}

#[test]
fn malformed_version_is_rejected() {
    let resp = send_recv(
        r#"{"protocol_version":"abc","id":1,"command":"handshake","capabilities":[]}"#,
    );
    assert_eq!(resp.status, "error");
    assert_eq!(resp.code.as_deref(), Some(ERR_VERSION_MISMATCH));
}
```

### 4. Existing test `version_mismatch_returns_error` (line 680)

This test sends `"99.0.0"` and expects an error — it should continue passing since major doesn't match.

## Files modified

- `shatter-rust/src/handler.rs` — add `is_version_compatible()`, `parse_major_minor()`, update `dispatch()`, add tests

## Verification

1. Write failing test, confirm it fails with `cargo test -p shatter-rust`
2. Apply the fix
3. `cargo test -p shatter-rust` — all pass
4. `cargo clippy -p shatter-rust -- -D warnings` — clean
