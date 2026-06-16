//! Google Gemini adapter for the [`SeedOracle`] trait.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};

use shatter_core::config::LlmConfig;
use shatter_core::oracle::{OracleContext, OracleResponse, SeedOracle};

use crate::parse::parse_response_structured;
use crate::prompt::{build_prompt, build_schema};

#[derive(Debug)]
pub struct GoogleAdapter {
    client: Client,
    model: String,
    api_key: String,
    llm: LlmConfig,
    base_url: String,
}

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";

impl GoogleAdapter {
    pub fn new(llm: LlmConfig) -> anyhow::Result<Self> {
        Self::with_base_url(llm, DEFAULT_BASE_URL)
    }

    pub(crate) fn with_base_url(llm: LlmConfig, base_url: &str) -> anyhow::Result<Self> {
        let adapter_cfg = llm.google.clone().unwrap_or_default();

        let api_key = adapter_cfg
            .api_key
            .or_else(|| std::env::var("SHATTER_GOOGLE_API_KEY").ok())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Google Gemini adapter requires an API key: set llm.google.api_key \
                     or SHATTER_GOOGLE_API_KEY env var"
                )
            })?;

        let client = Client::builder()
            .timeout(Duration::from_secs(u64::from(llm.timeout_seconds)))
            .build()?;

        Ok(Self {
            client,
            model: adapter_cfg.model,
            api_key,
            llm,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    fn endpoint(&self) -> String {
        format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url, self.model,
        )
    }

    fn build_request_body(&self, prompt: &str, schema: &Value) -> Value {
        json!({
            "contents": [{
                "parts": [{ "text": prompt }]
            }],
            "generationConfig": {
                "temperature": self.llm.temperature,
                "maxOutputTokens": self.llm.max_tokens_per_query,
                "responseMimeType": "application/json",
                "responseSchema": schema,
            }
        })
    }
}

#[async_trait]
impl SeedOracle for GoogleAdapter {
    async fn query(&self, ctx: OracleContext) -> anyhow::Result<OracleResponse> {
        let prompt = build_prompt(&ctx, self.llm.candidates_per_query);
        let schema = build_schema(&ctx.param_types);
        let body = self.build_request_body(&prompt, &schema);

        let url = self.endpoint();
        let resp = self
            .client
            .post(&url)
            .query(&[("key", &self.api_key)])
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
            return Err(crate::rate_limit::OracleError::RateLimit {
                status_code: 429,
                retry_after,
            }
            .into());
        }

        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Google Gemini auth failure (HTTP {status}): {text}");
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Google Gemini model not found (HTTP 404): {text}");
        }

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Google Gemini HTTP {status}: {text}");
        }

        let resp_body: Value = resp.json().await?;

        let text = resp_body["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or("");

        let parsed: Value = serde_json::from_str(text).unwrap_or(Value::Null);
        let candidates = parse_response_structured(parsed, &ctx.param_types, &ctx.attempted);

        let tokens_used = resp_body["usageMetadata"]["totalTokenCount"]
            .as_u64()
            .unwrap_or(text.len() as u64) as u32;

        Ok(OracleResponse {
            candidates,
            tokens_used,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shatter_core::config::GoogleAdapterConfig;
    use shatter_core::oracle::FailedCondition;
    use shatter_core::types::{ParamInfo, TypeInfo};
    use wiremock::matchers::{method, path_regex, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(api_key: &str) -> LlmConfig {
        LlmConfig {
            adapter: "google".into(),
            google: Some(GoogleAdapterConfig {
                model: "gemini-2.0-flash".into(),
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

    fn gemini_response(text: &str) -> Value {
        json!({
            "candidates": [{
                "content": {
                    "parts": [{ "text": text }],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        })
    }

    #[test]
    fn missing_api_key_errors() {
        // SAFETY: test is single-threaded for this env var.
        unsafe { std::env::remove_var("SHATTER_GOOGLE_API_KEY") };
        let cfg = LlmConfig {
            adapter: "google".into(),
            google: Some(GoogleAdapterConfig {
                model: "gemini-2.0-flash".into(),
                api_key: None,
            }),
            ..LlmConfig::default()
        };
        let err = GoogleAdapter::new(cfg).unwrap_err();
        assert!(err.to_string().contains("API key"));
    }

    #[test]
    fn endpoint_uses_model_name() {
        let cfg = test_config("test-key");
        let adapter = GoogleAdapter::with_base_url(cfg, "https://example.com").unwrap();
        assert_eq!(
            adapter.endpoint(),
            "https://example.com/v1beta/models/gemini-2.0-flash:generateContent"
        );
    }

    #[test]
    fn request_body_structure() {
        let cfg = test_config("test-key");
        let adapter = GoogleAdapter::new(cfg).unwrap();
        let schema = json!({"type": "array"});
        let body = adapter.build_request_body("test prompt", &schema);

        assert_eq!(body["contents"][0]["parts"][0]["text"], "test prompt");
        assert!(body["generationConfig"]["temperature"].as_f64().is_some());
        assert_eq!(
            body["generationConfig"]["responseMimeType"],
            "application/json"
        );
        assert_eq!(body["generationConfig"]["responseSchema"], schema);
    }

    #[tokio::test]
    async fn successful_query() {
        let server = MockServer::start().await;
        let resp_text = r#"[{"x": 42}, {"x": 99}]"#;

        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .and(query_param("key", "test-key"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(gemini_response(resp_text)),
            )
            .mount(&server)
            .await;

        let cfg = test_config("test-key");
        let adapter = GoogleAdapter::with_base_url(cfg, &server.uri()).unwrap();
        let resp = adapter.query(test_ctx()).await.unwrap();

        assert_eq!(resp.candidates.len(), 2);
        assert_eq!(resp.candidates[0], vec![serde_json::json!(42)]);
        assert_eq!(resp.candidates[1], vec![serde_json::json!(99)]);
        assert_eq!(resp.tokens_used, 15);
    }

    #[tokio::test]
    async fn rate_limit_returns_oracle_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .respond_with(
                ResponseTemplate::new(429)
                    .append_header("retry-after", "5"),
            )
            .mount(&server)
            .await;

        let cfg = test_config("test-key");
        let adapter = GoogleAdapter::with_base_url(cfg, &server.uri()).unwrap();
        let err = adapter.query(test_ctx()).await.unwrap_err();

        let oracle_err = err.downcast_ref::<crate::rate_limit::OracleError>().unwrap();
        match oracle_err {
            crate::rate_limit::OracleError::RateLimit {
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
    async fn auth_failure_returns_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .respond_with(
                ResponseTemplate::new(401).set_body_string("invalid api key"),
            )
            .mount(&server)
            .await;

        let cfg = test_config("bad-key");
        let adapter = GoogleAdapter::with_base_url(cfg, &server.uri()).unwrap();
        let err = adapter.query(test_ctx()).await.unwrap_err();

        assert!(err.to_string().contains("auth failure"));
        assert!(err.to_string().contains("401"));
    }

    #[tokio::test]
    async fn forbidden_returns_auth_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .respond_with(
                ResponseTemplate::new(403).set_body_string("forbidden"),
            )
            .mount(&server)
            .await;

        let cfg = test_config("bad-key");
        let adapter = GoogleAdapter::with_base_url(cfg, &server.uri()).unwrap();
        let err = adapter.query(test_ctx()).await.unwrap_err();

        assert!(err.to_string().contains("auth failure"));
        assert!(err.to_string().contains("403"));
    }

    #[tokio::test]
    async fn model_not_found_returns_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .respond_with(
                ResponseTemplate::new(404).set_body_string("model not found"),
            )
            .mount(&server)
            .await;

        let cfg = test_config("test-key");
        let adapter = GoogleAdapter::with_base_url(cfg, &server.uri()).unwrap();
        let err = adapter.query(test_ctx()).await.unwrap_err();

        assert!(err.to_string().contains("model not found"));
        assert!(err.to_string().contains("404"));
    }

    #[tokio::test]
    async fn malformed_response_returns_empty_candidates() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(gemini_response("not json at all")),
            )
            .mount(&server)
            .await;

        let cfg = test_config("test-key");
        let adapter = GoogleAdapter::with_base_url(cfg, &server.uri()).unwrap();
        let resp = adapter.query(test_ctx()).await.unwrap();

        assert!(resp.candidates.is_empty());
    }

    #[tokio::test]
    async fn generic_server_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .respond_with(
                ResponseTemplate::new(500).set_body_string("internal error"),
            )
            .mount(&server)
            .await;

        let cfg = test_config("test-key");
        let adapter = GoogleAdapter::with_base_url(cfg, &server.uri()).unwrap();
        let err = adapter.query(test_ctx()).await.unwrap_err();

        assert!(err.to_string().contains("500"));
    }

    #[tokio::test]
    async fn deduplicates_attempted_inputs() {
        let server = MockServer::start().await;
        let resp_text = r#"[{"x": 1}, {"x": 2}, {"x": 3}]"#;

        Mock::given(method("POST"))
            .and(path_regex(r"/v1beta/models/.+:generateContent"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(gemini_response(resp_text)),
            )
            .mount(&server)
            .await;

        let cfg = test_config("test-key");
        let adapter = GoogleAdapter::with_base_url(cfg, &server.uri()).unwrap();

        let mut ctx = test_ctx();
        ctx.attempted = vec![vec![serde_json::json!(2)]];

        let resp = adapter.query(ctx).await.unwrap();
        assert_eq!(resp.candidates.len(), 2);
        assert_eq!(resp.candidates[0], vec![serde_json::json!(1)]);
        assert_eq!(resp.candidates[1], vec![serde_json::json!(3)]);
    }

    #[test]
    fn api_key_from_config_preferred() {
        let cfg = test_config("from-config");
        let adapter = GoogleAdapter::new(cfg).unwrap();
        assert_eq!(adapter.api_key, "from-config");
    }
}
