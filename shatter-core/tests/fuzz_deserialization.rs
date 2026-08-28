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

const DEFAULT_FUZZ_CASES: u32 = 1000;
const FUZZ_CASES_ENV: &str = "SHATTER_FUZZ_CASES";
const MAX_INPUT_LEN: usize = 1024;

fn fuzz_config() -> ProptestConfig {
    ProptestConfig {
        cases: fuzz_cases(),
        ..ProptestConfig::default()
    }
}

fn fuzz_cases() -> u32 {
    let value = std::env::var(FUZZ_CASES_ENV).ok();
    parse_fuzz_cases(value.as_deref()).unwrap_or_else(|message| panic!("{message}"))
}

fn parse_fuzz_cases(value: Option<&str>) -> Result<u32, String> {
    let Some(value) = value else {
        return Ok(DEFAULT_FUZZ_CASES);
    };
    let cases = value
        .parse::<u32>()
        .map_err(|_| format!("{FUZZ_CASES_ENV} must be a positive integer, got {value:?}"))?;
    if cases == 0 {
        return Err(format!(
            "{FUZZ_CASES_ENV} must be a positive integer, got {value:?}"
        ));
    }
    Ok(cases)
}

#[cfg(test)]
mod case_tier_tests {
    use super::{DEFAULT_FUZZ_CASES, parse_fuzz_cases};

    #[test]
    fn fuzz_case_override_preserves_full_default() {
        assert_eq!(parse_fuzz_cases(None), Ok(DEFAULT_FUZZ_CASES));
        assert_eq!(parse_fuzz_cases(Some("32")), Ok(32));
    }

    #[test]
    fn fuzz_case_override_rejects_invalid_values() {
        assert!(parse_fuzz_cases(Some("0")).is_err());
        assert!(parse_fuzz_cases(Some("many")).is_err());
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
