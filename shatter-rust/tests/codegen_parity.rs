//! Drift-detection tests between hand-rolled protocol vocabulary in
//! `shatter_rust::protocol` and the codegen-emitted vocabulary in
//! `shatter_rust::generated::protocol_enums` (str-1hlk.9).
//!
//! These tests are the wiring that makes registry → Rust drift fail at
//! `cargo test` time. The codegen `--check` mode catches drift between
//! `protocol/registry.yaml` and the generated module. These tests catch
//! drift between the generated module and the hand-rolled constants the
//! handler/wire-format actually depend on.

use shatter_rust::generated::protocol_enums as enums;
use shatter_rust::protocol;

#[test]
fn protocol_version_matches_generated() {
    assert_eq!(protocol::PROTOCOL_VERSION, enums::PROTOCOL_VERSION);
}

#[test]
fn hand_rolled_error_codes_match_generated() {
    // Hand-rolled `ALL_ERROR_CODES` in protocol.rs is what handler error
    // responses use. The generated slice is what the registry says exists.
    // If these ever drift, a registry update silently fails to surface a
    // new error code in the Rust frontend.
    let mut hand_rolled: Vec<&str> = protocol::ALL_ERROR_CODES.to_vec();
    hand_rolled.sort_unstable();
    let mut generated: Vec<&str> = enums::ALL_ERROR_CODES.to_vec();
    generated.sort_unstable();
    assert_eq!(
        hand_rolled, generated,
        "hand-rolled ALL_ERROR_CODES drifted from generated::ALL_ERROR_CODES; \
         update protocol.rs to match registry.yaml or run \
         `python3 scripts/protocol-codegen.py --write` to refresh the binding"
    );
}

#[test]
fn generated_slices_are_sorted_and_nonempty() {
    // Codegen contract: every slice is sorted ASCII and non-empty.
    let cases: &[(&str, &[&str])] = &[
        ("ALL_COMMANDS", enums::ALL_COMMANDS),
        ("ALL_RESPONSE_STATUSES", enums::ALL_RESPONSE_STATUSES),
        ("ALL_ERROR_CODES", enums::ALL_ERROR_CODES),
        ("ALL_SETUP_LEVELS", enums::ALL_SETUP_LEVELS),
        ("ALL_GENERATOR_KINDS", enums::ALL_GENERATOR_KINDS),
        ("ALL_BRANCH_TYPES", enums::ALL_BRANCH_TYPES),
    ];
    for (name, values) in cases.iter().copied() {
        assert!(!values.is_empty(), "{name} unexpectedly empty");
        let mut sorted = values.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, values, "{name} not sorted");
    }
}

#[test]
fn response_statuses_include_universal_error() {
    assert!(
        enums::ALL_RESPONSE_STATUSES.contains(&"error"),
        "universal `error` status missing from generated ALL_RESPONSE_STATUSES"
    );
}
