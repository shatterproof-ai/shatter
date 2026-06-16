//! Anthropic (Claude) adapter for the [`SeedOracle`] trait.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};

use shatter_core::config::LlmConfig;
use shatter_core::oracle::{OracleContext, OracleResponse, SeedOracle};

use crate::parse::parse_response_structured;
use crate::prompt::{build_prompt, build_schema};
use crate::rate_limit::OracleError;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

pub struct AnthropicAdapter {
    client: Client,
    api_key: String,
    model: String,
    llm: LlmConfig,
}

impl AnthropicAdapter {
    pub fn new(llm: &LlmConfig) -> anyhow::Result<Self> {
        let adapter_cfg = llm.anthropic.clone().unwrap_or_default();

        let api_key = adapter_cfg
            .api_key
            .or_else(|| std::env::var("SHATTER_ANTHROPIC_API_KEY").ok())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Anthropic adapter requires an API key: set llm.anthropic.api_key \
                     or SHATTER_ANTHROPIC_API_KEY"
                )
            })?;

        let client = Client::builder()
            .timeout(Duration::from_secs(u64::from(llm.timeout_seconds)))
            .build()?;

        Ok(Self {
            client,
            api_key,
            model: adapter_cfg.model,
            llm: llm.clone(),
        })
    }

    fn build_request_body(&self, ctx: &OracleContext) -> Value {
        let prompt = build_prompt(ctx, self.llm.candidates_per_query);
        let schema = build_schema(&ctx.param_types);

        json!({
            "model": self.model,
            "max_tokens": self.llm.max_tokens_per_query,
            "temperature": self.llm.temperature,
            "system": [{
                "type": "text",
                "text": "You are a test input generator for a concolic execution engine. \
                         Respond only with the tool call, no prose.",
                "cache_control": { "type": "ephemeral" }
            }],
            "messages": [{
                "role": "user",
                "content": prompt
            }],
            "tools": [{
                "name": "suggest_inputs",
                "description": "Return candidate input vectors for the target function.",
                "input_schema": schema
            }],
            "tool_choice": { "type": "tool", "name": "suggest_inputs" }
        })
    }

    async fn send_request(&self, url: &str, body: Value) -> anyhow::Result<OracleResponse> {
        let resp = self
            .client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs);
            return Err(OracleError::RateLimit {
                status_code: 429,
                retry_after,
            }
            .into());
        }

        if status == reqwest::StatusCode::UNAUTHORIZED {
            anyhow::bail!("Anthropic API authentication failed (401): check your API key");
        }

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            if let Ok(body) = serde_json::from_str::<Value>(&text) {
                let error_type = body["error"]["type"].as_str().unwrap_or("");
                if error_type == "overloaded_error" || error_type == "rate_limit_error" {
                    return Err(OracleError::RateLimit {
                        status_code: status.as_u16(),
                        retry_after: None,
                    }
                    .into());
                }
            }
            anyhow::bail!("Anthropic API HTTP {status}: {text}");
        }

        let resp_body: Value = resp.json().await?;
        self.extract_candidates(&resp_body)
    }

    fn extract_candidates(&self, resp_body: &Value) -> anyhow::Result<OracleResponse> {
        let usage = &resp_body["usage"];
        let tokens_used = usage["input_tokens"].as_u64().unwrap_or(0)
            + usage["output_tokens"].as_u64().unwrap_or(0);

        let content = resp_body["content"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing content array in Anthropic response"))?;

        for block in content {
            if block["type"].as_str() == Some("tool_use") && block["name"].as_str() == Some("suggest_inputs") {
                let input = block["input"].clone();
                let candidates = parse_response_structured(input, &[], &[]);
                return Ok(OracleResponse {
                    candidates,
                    tokens_used: tokens_used as u32,
                });
            }
        }

        anyhow::bail!("no tool_use block found in Anthropic response")
    }
}

#[async_trait]
impl SeedOracle for AnthropicAdapter {
    async fn query(&self, ctx: OracleContext) -> anyhow::Result<OracleResponse> {
        let body = self.build_request_body(&ctx);
        let mut resp = self.send_request(API_URL, body).await?;
        // Re-validate candidates with actual param_types (extract_candidates
        // used empty types for the initial parse since it doesn't have ctx).
        resp.candidates = parse_response_structured(
            json!(resp.candidates.iter().map(|c| {
                let mut obj = serde_json::Map::new();
                for (i, p) in ctx.param_types.iter().enumerate() {
                    if let Some(v) = c.get(i) {
                        obj.insert(p.name.clone(), v.clone());
                    }
                }
                Value::Object(obj)
            }).collect::<Vec<_>>()),
            &ctx.param_types,
            &ctx.attempted,
        );
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use shatter_core::config::AnthropicAdapterConfig;
    use shatter_core::oracle::FailedCondition;
    use shatter_core::types::{ParamInfo, TypeInfo};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(api_key: &str) -> LlmConfig {
        LlmConfig {
            adapter: "anthropic".into(),
            anthropic: Some(AnthropicAdapterConfig {
                model: "claude-sonnet-4-6".into(),
                api_key: Some(api_key.into()),
            }),
            ..LlmConfig::default()
        }
    }

    fn test_ctx() -> OracleContext {
        OracleContext {
            function_source: "fn f(x: i64) -> bool { x > 10 }".into(),
            param_types: vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int { int_width: None, int_signed: None },
                type_name: None,
            }],
            condition: FailedCondition {
                predicate: "x > 10".into(),
                location: "test.rs:1".into(),
            },
            attempted: vec![],
        }
    }

    fn success_response() -> Value {
        json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": "toolu_test",
                "name": "suggest_inputs",
                "input": [{"x": 42}, {"x": 100}]
            }],
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50
            }
        })
    }

    fn make_adapter(_server: &MockServer) -> AnthropicAdapter {
        let cfg = test_config("test-key");
        AnthropicAdapter::new(&cfg).unwrap()
    }

    fn server_url(server: &MockServer) -> String {
        format!("{}/v1/messages", server.uri())
    }

    #[tokio::test]
    async fn successful_query_returns_candidates() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "test-key"))
            .and(header("anthropic-version", API_VERSION))
            .respond_with(ResponseTemplate::new(200).set_body_json(success_response()))
            .mount(&server)
            .await;

        let adapter = make_adapter(&server);
        let body = adapter.build_request_body(&test_ctx());
        let resp = adapter.send_request(&server_url(&server), body).await.unwrap();
        assert_eq!(resp.tokens_used, 150);
        assert!(!resp.candidates.is_empty());
    }

    #[tokio::test]
    async fn auth_failure_returns_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(401)
                    .set_body_json(json!({"error": {"type": "authentication_error", "message": "invalid api key"}})),
            )
            .mount(&server)
            .await;

        let adapter = make_adapter(&server);
        let body = adapter.build_request_body(&test_ctx());
        let err = adapter.send_request(&server_url(&server), body).await.unwrap_err();
        assert!(err.to_string().contains("authentication failed"));
    }

    #[tokio::test]
    async fn rate_limit_429_returns_oracle_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("retry-after", "5")
                    .set_body_json(json!({"error": {"type": "rate_limit_error", "message": "rate limited"}})),
            )
            .mount(&server)
            .await;

        let adapter = make_adapter(&server);
        let body = adapter.build_request_body(&test_ctx());
        let err = adapter.send_request(&server_url(&server), body).await.unwrap_err();
        let oracle_err = err.downcast_ref::<OracleError>().expect("should be OracleError");
        match oracle_err {
            OracleError::RateLimit { status_code, retry_after } => {
                assert_eq!(status_code, &429);
                assert_eq!(retry_after, &Some(Duration::from_secs(5)));
            }
            _ => panic!("expected RateLimit variant"),
        }
    }

    #[tokio::test]
    async fn overloaded_error_in_body_returns_rate_limit() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(
                ResponseTemplate::new(529)
                    .set_body_json(json!({"error": {"type": "overloaded_error", "message": "overloaded"}})),
            )
            .mount(&server)
            .await;

        let adapter = make_adapter(&server);
        let body = adapter.build_request_body(&test_ctx());
        let err = adapter.send_request(&server_url(&server), body).await.unwrap_err();
        let oracle_err = err.downcast_ref::<OracleError>().expect("should be OracleError");
        assert!(matches!(oracle_err, OracleError::RateLimit { .. }));
    }

    #[test]
    fn missing_api_key_errors() {
        let cfg = LlmConfig {
            adapter: "anthropic".into(),
            anthropic: Some(AnthropicAdapterConfig {
                model: "claude-sonnet-4-6".into(),
                api_key: None,
            }),
            ..LlmConfig::default()
        };
        // Clear env var to ensure the fallback path is tested.
        // SAFETY: this test does not run concurrently with other tests that
        // depend on this env var.
        unsafe { std::env::remove_var("SHATTER_ANTHROPIC_API_KEY") };
        let result = AnthropicAdapter::new(&cfg);
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("API key"));
    }

    #[test]
    fn request_body_structure() {
        let cfg = test_config("test-key");
        let adapter = AnthropicAdapter::new(&cfg).unwrap();
        let body = adapter.build_request_body(&test_ctx());

        assert_eq!(body["model"], "claude-sonnet-4-6");
        assert!(body["tools"].as_array().unwrap().len() == 1);
        assert_eq!(body["tools"][0]["name"], "suggest_inputs");
        assert_eq!(body["tool_choice"]["type"], "tool");
        assert_eq!(body["tool_choice"]["name"], "suggest_inputs");
        assert!(body["system"][0]["cache_control"]["type"].as_str() == Some("ephemeral"));
    }

    #[test]
    fn extract_candidates_from_tool_use() {
        let resp = success_response();
        let cfg = test_config("test-key");
        let adapter = AnthropicAdapter::new(&cfg).unwrap();
        let result = adapter.extract_candidates(&resp).unwrap();
        assert_eq!(result.tokens_used, 150);
    }

    #[test]
    fn extract_candidates_missing_tool_use_errors() {
        let resp = json!({
            "content": [{"type": "text", "text": "hello"}],
            "usage": {"input_tokens": 10, "output_tokens": 5}
        });
        let cfg = test_config("test-key");
        let adapter = AnthropicAdapter::new(&cfg).unwrap();
        let err = adapter.extract_candidates(&resp).unwrap_err();
        assert!(err.to_string().contains("no tool_use block"));
    }

    #[test]
    fn default_config_uses_correct_model() {
        let cfg = AnthropicAdapterConfig::default();
        assert_eq!(cfg.model, "claude-sonnet-4-6");
        assert!(cfg.api_key.is_none());
    }
}
