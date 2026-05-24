//! Shared LLM seed-oracle plumbing for Shatter: prompt construction, response
//! parsing/validation, a trait-level mock oracle for unit tests, and a
//! rate-limiting wrapper used by all provider adapters.
//!
//! Real provider adapters (Anthropic, OpenAI, Google, custom) live in
//! separate modules/issues and depend on this crate.

pub mod mock;
pub mod parse;
pub mod prompt;
pub mod rate_limit;

pub use mock::MockSeedOracle;
pub use parse::{parse_response, parse_response_structured};
pub use prompt::{build_prompt, build_schema};
pub use rate_limit::{OracleError, RateLimitedOracle};
