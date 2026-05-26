//! Adapter registry: config-driven construction of [`SeedOracle`] instances.

use std::sync::Arc;

use shatter_core::config::LlmConfig;
use shatter_core::oracle::SeedOracle;

use crate::custom_http::CustomHttpAdapter;
use crate::local_model::LocalModelAdapter;
use crate::rate_limit::RateLimitedOracle;

/// Construct a [`SeedOracle`] from the config's `adapter` field.
///
/// Recognized adapters:
/// - `"custom"` → [`CustomHttpAdapter`]
/// - `"local"`  → [`LocalModelAdapter`]
/// - `"anthropic"` / `"openai"` / `"google"` → stub (not-yet-implemented)
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
        "anthropic" | "openai" | "google" => {
            anyhow::bail!(
                "adapter {:?} is not yet implemented — \
                 see str-9w8 (Anthropic), str-0o8 (OpenAI), str-w4c (Google)",
                config.adapter
            );
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
        for name in &["anthropic", "openai", "google"] {
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
