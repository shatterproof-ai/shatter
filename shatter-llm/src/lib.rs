//! LLM seed-oracle plumbing for Shatter: prompt construction, response
//! parsing/validation, adapter implementations, a registry, a trait-level
//! mock oracle for unit tests, and a rate-limiting wrapper.

pub mod anthropic;
pub mod custom_http;
pub mod google;
pub mod local_model;
pub mod mock;
pub mod parse;
pub mod prompt;
pub mod rate_limit;
pub mod registry;

pub use anthropic::AnthropicAdapter;
pub use custom_http::CustomHttpAdapter;
pub use google::GoogleAdapter;
pub use local_model::LocalModelAdapter;
pub use mock::MockSeedOracle;
pub use parse::{parse_response, parse_response_structured};
pub use prompt::{build_prompt, build_schema};
pub use rate_limit::{OracleError, RateLimitedOracle};
pub use registry::build_oracle;
