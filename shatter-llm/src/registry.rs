//! Adapter registry: config-driven construction of [`SeedOracle`] instances.

use std::sync::Arc;

use shatter_core::config::LlmConfig;
use shatter_core::oracle::SeedOracle;

use crate::anthropic::AnthropicAdapter;
use crate::custom_http::CustomHttpAdapter;
use crate::google::GoogleAdapter;
use crate::local_model::LocalModelAdapter;
use crate::openai::OpenAiAdapter;
use crate::rate_limit::RateLimitedOracle;

/// Construct a [`SeedOracle`] from the config's `adapter` field.
///
/// Recognized adapters:
/// - `"custom"` → [`CustomHttpAdapter`]
/// - `"local"`  → [`LocalModelAdapter`]
/// - `"anthropic"` → [`AnthropicAdapter`]
/// - `"openai"` → [`OpenAiAdapter`]
/// - `"google"` → [`GoogleAdapter`]
/// - `"none"` → error (oracle explicitly disabled)
/// - anything else → error (unknown adapter)
///
/// The returned oracle is wrapped in [`RateLimitedOracle`] using `max_retries`
/// from the config.
pub fn build_oracle(config: &LlmConfig) -> anyhow::Result<Arc<dyn SeedOracle>> {
    let max_retries = config.max_retries;

    let oracle: Arc<dyn SeedOracle> = match config.adapter.as_str() {
        "custom" => {
            let inner = CustomHttpAdapter::new(config.clone())?;
            Arc::new(RateLimitedOracle::new(inner, max_retries))
        }
        "local" => {
            let inner = LocalModelAdapter::new(config.clone())?;
            Arc::new(RateLimitedOracle::new(inner, max_retries))
        }
        "anthropic" => {
            let inner = AnthropicAdapter::new(config)?;
            Arc::new(RateLimitedOracle::new(inner, max_retries))
        }
        "openai" => {
            let inner = OpenAiAdapter::new(config.clone())?;
            Arc::new(RateLimitedOracle::new(inner, max_retries))
        }
        "google" => {
            let inner = GoogleAdapter::new(config.clone())?;
            Arc::new(RateLimitedOracle::new(inner, max_retries))
        }
        "none" => {
            anyhow::bail!("adapter \"none\" selected — LLM oracle is disabled");
        }
        other => {
            anyhow::bail!(
                "unknown LLM adapter {:?}; valid adapters: \
                 custom, local, anthropic, openai, google, none",
                other
            );
        }
    };

    Ok(oracle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shatter_core::config::{CustomAdapterConfig, CustomAuthMode, LocalAdapterConfig};

    fn expect_err(config: &LlmConfig) -> anyhow::Error {
        match build_oracle(config) {
            Err(e) => e,
            Ok(_) => panic!("expected build_oracle to fail for adapter {:?}", config.adapter),
        }
    }

    #[test]
    fn build_custom_adapter() {
        let config = LlmConfig {
            adapter: "custom".into(),
            custom: Some(CustomAdapterConfig {
                url: "http://localhost:8080/v1/chat".into(),
                headers: Default::default(),
                auth: CustomAuthMode::None,
                request_path: "$.prompt".into(),
                response_path: "$.text".into(),
            }),
            ..LlmConfig::default()
        };
        assert!(build_oracle(&config).is_ok());
    }

    #[test]
    fn build_custom_without_config_errors() {
        let config = LlmConfig {
            adapter: "custom".into(),
            ..LlmConfig::default()
        };
        let err = expect_err(&config);
        assert!(err.to_string().contains("llm.custom config required"));
    }

    #[test]
    fn build_local_adapter() {
        let config = LlmConfig {
            adapter: "local".into(),
            local: Some(LocalAdapterConfig {
                command: vec!["echo".into()],
                model: "test-model".into(),
                port: 11434,
                startup_timeout_seconds: 5,
            }),
            ..LlmConfig::default()
        };
        assert!(build_oracle(&config).is_ok());
    }

    #[test]
    fn stub_adapters_return_not_implemented() {
        for name in &[] as &[&str] {
            let config = LlmConfig {
                adapter: name.to_string(),
                ..LlmConfig::default()
            };
            let err = expect_err(&config);
            assert!(
                err.to_string().contains("not yet implemented"),
                "expected not-implemented error for {name}, got: {err}"
            );
        }
    }

    #[test]
    fn anthropic_adapter_requires_api_key() {
        let config = LlmConfig {
            adapter: "anthropic".into(),
            ..LlmConfig::default()
        };
        unsafe { std::env::remove_var("SHATTER_ANTHROPIC_API_KEY") };
        let err = expect_err(&config);
        assert!(
            err.to_string().contains("API key"),
            "expected API key error, got: {err}"
        );
    }

    #[test]
    fn google_adapter_requires_api_key() {
        unsafe { std::env::remove_var("SHATTER_GOOGLE_API_KEY") };
        let config = LlmConfig {
            adapter: "google".into(),
            ..LlmConfig::default()
        };
        let err = expect_err(&config);
        assert!(
            err.to_string().contains("API key"),
            "expected API key error, got: {err}"
        );
    }

    #[test]
    fn build_openai_without_key_errors() {
        let config = LlmConfig {
            adapter: "openai".into(),
            ..LlmConfig::default()
        };
        unsafe { std::env::remove_var("SHATTER_OPENAI_API_KEY") };
        let err = expect_err(&config);
        assert!(err.to_string().contains("API key required"));
    }

    #[test]
    fn none_adapter_errors() {
        let config = LlmConfig {
            adapter: "none".into(),
            ..LlmConfig::default()
        };
        let err = expect_err(&config);
        assert!(err.to_string().contains("disabled"));
    }

    #[test]
    fn unknown_adapter_errors() {
        let config = LlmConfig {
            adapter: "magic-llm".into(),
            ..LlmConfig::default()
        };
        let err = expect_err(&config);
        assert!(err.to_string().contains("unknown LLM adapter"));
    }
}
