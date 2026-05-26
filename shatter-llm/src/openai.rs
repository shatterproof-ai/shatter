//! OpenAI Chat Completions adapter for the [`SeedOracle`] trait.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};

use shatter_core::config::LlmConfig;
use shatter_core::oracle::{OracleContext, OracleResponse, SeedOracle};

use crate::parse::parse_response_structured;
use crate::prompt::{build_prompt, build_schema};
use crate::rate_limit::OracleError;

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

#[derive(Debug)]
pub struct OpenAiAdapter {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
    llm: LlmConfig,
}

impl OpenAiAdapter {
    pub fn new(llm: LlmConfig) -> anyhow::Result<Self> {
        let openai_cfg = llm.openai.clone().unwrap_or_default();

        let api_key = openai_cfg
            .api_key
            .or_else(|| std::env::var("SHATTER_OPENAI_API_KEY").ok())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "OpenAI API key required: set llm.openai.api_key or SHATTER_OPENAI_API_KEY"
                )
            })?;

        let base_url = openai_cfg
            .base_url
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        let client = Client::builder()
            .timeout(Duration::from_secs(u64::from(llm.timeout_seconds)))
            .build()?;

        Ok(Self {
            client,
            api_key,
            model: openai_cfg.model,
            base_url,
            llm,
        })
    }

    fn endpoint(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        format!("{base}/chat/completions")
    }

    fn build_request_body(&self, prompt: &str, schema: &Value) -> Value {
        json!({
            "model": self.model,
            "temperature": self.llm.temperature,
            "max_tokens": self.llm.max_tokens_per_query,
            "messages": [
                {
                    "role": "system",
                    "content": "You are a test input generator. Respond only with the requested JSON."
                },
                {
                    "role": "user",
                    "content": prompt
                }
            ],
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "seed_candidates",
                    "strict": true,
                    "schema": {
                        "type": "object",
                        "properties": {
                            "candidates": schema
                        },
                        "required": ["candidates"],
                        "additionalProperties": false
                    }
                }
            }
        })
    }
}

#[async_trait]
impl SeedOracle for OpenAiAdapter {
    async fn query(&self, ctx: OracleContext) -> anyhow::Result<OracleResponse> {
        let prompt = build_prompt(&ctx, self.llm.candidates_per_query);
        let schema = build_schema(&ctx.param_types);
        let body = self.build_request_body(&prompt, &schema);

        let resp = self
            .client
            .post(self.endpoint())
            .bearer_auth(&self.api_key)
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
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI auth failed (401): {text}");
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI model not found (404): {text}");
        }

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI HTTP {status}: {text}");
        }

        let resp_body: Value = resp.json().await?;

        let content_str = resp_body["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("");

        let tokens_used = resp_body["usage"]["total_tokens"]
            .as_u64()
            .unwrap_or(content_str.len() as u64) as u32;

        let parsed: Value =
            serde_json::from_str(content_str).unwrap_or(Value::Array(Vec::new()));

        let candidates_value = if parsed.is_object() {
            parsed
                .get("candidates")
                .cloned()
                .unwrap_or(Value::Array(Vec::new()))
        } else {
            parsed
        };

        let candidates =
            parse_response_structured(candidates_value, &ctx.param_types, &ctx.attempted);

        Ok(OracleResponse {
            candidates,
            tokens_used,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shatter_core::config::OpenAiAdapterConfig;
    use shatter_core::oracle::FailedCondition;
    use shatter_core::types::{ParamInfo, TypeInfo};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_ctx() -> OracleContext {
        OracleContext {
            function_source: "fn f(x: i64) -> bool { x > 10 }".into(),
            param_types: vec![ParamInfo {
                name: "x".into(),
                typ: TypeInfo::Int,
                type_name: None,
            }],
            condition: FailedCondition {
                predicate: "x > 10".into(),
                location: "test.rs:1".into(),
            },
            attempted: vec![],
        }
    }

    fn make_config(base_url: &str) -> LlmConfig {
        LlmConfig {
            adapter: "openai".into(),
            openai: Some(OpenAiAdapterConfig {
                model: "gpt-4o".into(),
                api_key: Some("test-key-123".into()),
                base_url: Some(base_url.into()),
            }),
            ..LlmConfig::default()
        }
    }

    fn success_body(content: &str, total_tokens: u32) -> Value {
        json!({
            "id": "chatcmpl-test",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": content
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": total_tokens.saturating_sub(10),
                "total_tokens": total_tokens
            }
        })
    }

    #[tokio::test]
    async fn successful_query_returns_candidates() {
        let server = MockServer::start().await;
        let content = r#"{"candidates": [{"x": 42}, {"x": 99}]}"#;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer test-key-123"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(success_body(content, 150)),
            )
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let adapter = OpenAiAdapter::new(config).unwrap();
        let resp = adapter.query(make_ctx()).await.unwrap();

        assert_eq!(resp.candidates.len(), 2);
        assert_eq!(resp.candidates[0], vec![serde_json::json!(42)]);
        assert_eq!(resp.candidates[1], vec![serde_json::json!(99)]);
        assert_eq!(resp.tokens_used, 150);
    }

    #[tokio::test]
    async fn rate_limit_429_returns_oracle_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(429)
                    .append_header("retry-after", "5")
                    .set_body_string("rate limited"),
            )
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let adapter = OpenAiAdapter::new(config).unwrap();
        let err = adapter.query(make_ctx()).await.unwrap_err();

        let oracle_err = err.downcast_ref::<OracleError>().expect("should be OracleError");
        match oracle_err {
            OracleError::RateLimit {
                status_code,
                retry_after,
            } => {
                assert_eq!(*status_code, 429);
                assert_eq!(*retry_after, Some(Duration::from_secs(5)));
            }
            _ => panic!("expected RateLimit variant"),
        }
    }

    #[tokio::test]
    async fn auth_failure_401_returns_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(401).set_body_string("invalid api key"),
            )
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let adapter = OpenAiAdapter::new(config).unwrap();
        let err = adapter.query(make_ctx()).await.unwrap_err();
        assert!(err.to_string().contains("auth failed (401)"));
    }

    #[tokio::test]
    async fn model_not_found_404_returns_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(404).set_body_string("model not found"),
            )
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let adapter = OpenAiAdapter::new(config).unwrap();
        let err = adapter.query(make_ctx()).await.unwrap_err();
        assert!(err.to_string().contains("model not found (404)"));
    }

    #[tokio::test]
    async fn custom_base_url_is_used() {
        let server = MockServer::start().await;
        let content = r#"{"candidates": [{"x": 7}]}"#;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(success_body(content, 50)),
            )
            .expect(1)
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let adapter = OpenAiAdapter::new(config).unwrap();
        let resp = adapter.query(make_ctx()).await.unwrap();
        assert_eq!(resp.candidates.len(), 1);
    }

    #[test]
    fn missing_api_key_errors() {
        let config = LlmConfig {
            adapter: "openai".into(),
            openai: Some(OpenAiAdapterConfig {
                model: "gpt-4o".into(),
                api_key: None,
                base_url: None,
            }),
            ..LlmConfig::default()
        };
        unsafe { std::env::remove_var("SHATTER_OPENAI_API_KEY") };
        let err = OpenAiAdapter::new(config).unwrap_err();
        assert!(err.to_string().contains("API key required"));
    }

    #[test]
    fn api_key_from_env() {
        unsafe { std::env::set_var("SHATTER_OPENAI_API_KEY", "env-key-456") };
        let config = LlmConfig {
            adapter: "openai".into(),
            openai: Some(OpenAiAdapterConfig {
                model: "gpt-4o".into(),
                api_key: None,
                base_url: Some("http://localhost:1234".into()),
            }),
            ..LlmConfig::default()
        };
        let adapter = OpenAiAdapter::new(config).unwrap();
        assert_eq!(adapter.api_key, "env-key-456");
        unsafe { std::env::remove_var("SHATTER_OPENAI_API_KEY") };
    }

    #[tokio::test]
    async fn structured_output_with_wrapper_object() {
        let server = MockServer::start().await;
        let content = r#"{"candidates": [{"x": 11}, {"x": 20}]}"#;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(success_body(content, 120)),
            )
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let adapter = OpenAiAdapter::new(config).unwrap();
        let resp = adapter.query(make_ctx()).await.unwrap();
        assert_eq!(resp.candidates.len(), 2);
        assert_eq!(resp.candidates[0], vec![serde_json::json!(11)]);
    }

    #[tokio::test]
    async fn malformed_content_returns_empty_candidates() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(success_body("not valid json {{", 50)),
            )
            .mount(&server)
            .await;

        let config = make_config(&server.uri());
        let adapter = OpenAiAdapter::new(config).unwrap();
        let resp = adapter.query(make_ctx()).await.unwrap();
        assert!(resp.candidates.is_empty());
    }
}
