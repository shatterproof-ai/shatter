// str-difh: Repo-local Rust fixture for the missing-frontend warning test
// and for ad-hoc local verification that a built `shatter-rust` on PATH
// discovers Rust files/functions.
//
// Kept deliberately small (a single free function with one branch) so:
//   - Discovery treats it as exactly one Rust file with one function.
//   - When `shatter-rust` is absent, the CLI emits one
//     `skipping 1 Rust file(s): shatter-rust frontend not found ...` warning.
//   - When `shatter-rust` is present, exploration completes quickly.

/// Absolute difference between two integers. The `if a >= b` branch makes
/// this a useful target for verifying that Rust function/branch discovery
/// is wired up when running `shatter scan` with `shatter-rust` on PATH.
pub fn abs_diff(a: i64, b: i64) -> i64 {
    if a >= b { a - b } else { b - a }
}
