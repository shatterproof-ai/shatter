//! Byte-level fuzz targets for deserialization boundaries.
//!
//! Feeds arbitrary byte vectors to every `serde_json::from_slice` and
//! `serde_yaml::from_slice` entry point in shatter-core. The only assertion is
//! "does not panic" — a returned `Err` is fine; an unwinding panic is a bug.
//!
//! These use proptest (not cargo-fuzz) so they run in CI without nightly.
//! For deeper coverage-guided fuzzing, consider adding cargo-fuzz targets later.

use proptest::prelude::*;

use shatter_core::config::ShatterConfig;
use shatter_core::protocol::{Request, Response};
use shatter_core::spec::FunctionSpec;
use shatter_core::sym_expr::SymExpr;
use shatter_core::types::TypeInfo;

const FUZZ_CASES: u32 = 1000;
const MAX_INPUT_LEN: usize = 1024;

fn fuzz_config() -> ProptestConfig {
    ProptestConfig {
        cases: FUZZ_CASES,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(fuzz_config())]

    /// Arbitrary bytes fed to `Request` JSON deserialization must not panic.
    #[test]
    fn fuzz_request_json(bytes in prop::collection::vec(any::<u8>(), 0..MAX_INPUT_LEN)) {
        let _ = serde_json::from_slice::<Request>(&bytes);
    }

    /// Arbitrary bytes fed to `Response` JSON deserialization must not panic.
    #[test]
    fn fuzz_response_json(bytes in prop::collection::vec(any::<u8>(), 0..MAX_INPUT_LEN)) {
        let _ = serde_json::from_slice::<Response>(&bytes);
    }

    /// Arbitrary bytes fed to `SymExpr` JSON deserialization must not panic.
    #[test]
    fn fuzz_symexpr_json(bytes in prop::collection::vec(any::<u8>(), 0..MAX_INPUT_LEN)) {
        let _ = serde_json::from_slice::<SymExpr>(&bytes);
    }

    /// Arbitrary bytes fed to `TypeInfo` JSON deserialization must not panic.
    #[test]
    fn fuzz_typeinfo_json(bytes in prop::collection::vec(any::<u8>(), 0..MAX_INPUT_LEN)) {
        let _ = serde_json::from_slice::<TypeInfo>(&bytes);
    }

    /// Arbitrary bytes fed to `FunctionSpec` YAML deserialization must not panic.
    #[test]
    fn fuzz_function_spec_yaml(bytes in prop::collection::vec(any::<u8>(), 0..MAX_INPUT_LEN)) {
        let _ = serde_yaml::from_slice::<FunctionSpec>(&bytes);
    }

    /// Arbitrary bytes fed to `ShatterConfig` YAML deserialization must not panic.
    #[test]
    fn fuzz_shatter_config_yaml(bytes in prop::collection::vec(any::<u8>(), 0..MAX_INPUT_LEN)) {
        let _ = serde_yaml::from_slice::<ShatterConfig>(&bytes);
    }
}
