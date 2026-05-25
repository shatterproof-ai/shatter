//! Integration tests for the LLM oracle HTTP adapter pipeline.
//!
//! Uses wiremock as the HTTP mock server. Since no real provider adapter has
//! landed yet, a minimal StubHttpAdapter exercises the HTTP + parse pipeline.
//! Remove StubHttpAdapter after str-g5b lands.

use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use shatter_core::oracle::{
    ConditionId, FailedCondition, InputVector, OracleContext, OracleResponse, SeedOracle,
};
use shatter_llm::{OracleError, RateLimitedOracle, parse_response};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Minimal stub HTTP adapter for testing the HTTP + parse pipeline.
/// Takes a base URL, POSTs `{"prompt": "<text>"}`, expects a JSON array
/// response. Not production-quality — exists only to exercise the pipeline.
#[cfg(test)]
struct StubHttpAdapter {
    base_url: String,
    client: reqwest::Client,
}

impl StubHttpAdapter {
    fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl SeedOracle for StubHttpAdapter {
    async fn query(&self, ctx: OracleContext) -> anyhow::Result<OracleResponse> {
        let prompt = shatter_llm::build_prompt(&ctx, 3);
        let resp = self
            .client
            .post(format!("{}/v1/generate", self.base_url))
            .json(&json!({ "prompt": prompt }))
            .timeout(Duration::from_secs(5))
            .send()
            .await?;

        let status = resp.status().as_u16();
        match status {
            200 => {}
            401 => return Err(anyhow::anyhow!("auth error: 401 Unauthorized")),
            429 => {
                return Err(OracleError::RateLimit {
                    status_code: 429,
                    retry_after: Some(Duration::from_millis(10)),
                }
                .into());
            }
            other => return Err(anyhow::anyhow!("unexpected status: {other}")),
        }

        let body = resp.text().await?;
        let candidates = parse_response(&body, &ctx.param_types, &ctx.attempted);
        Ok(OracleResponse {
            candidates,
            tokens_used: body.len() as u32,
        })
    }
}

fn make_ctx() -> OracleContext {
    use shatter_core::types::{ParamInfo, TypeInfo};
    OracleContext {
        function_source: "fn foo(x: i32) -> bool { x > 10 }".to_string(),
        param_types: vec![ParamInfo {
            name: "x".to_string(),
            typ: TypeInfo::Int,
            type_name: None,
        }],
        condition: FailedCondition {
            predicate: "x > 10".to_string(),
            location: "test.rs:1".to_string(),
        },
        attempted: vec![],
    }
}

#[tokio::test]
async fn well_formed_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"x": 11},
            {"x": 99}
        ])))
        .mount(&server)
        .await;

    let adapter = StubHttpAdapter::new(&server.uri());
    let resp = adapter.query(make_ctx()).await.unwrap();
    assert_eq!(resp.candidates.len(), 2);
    assert_eq!(resp.candidates[0], vec![json!(11)]);
    assert_eq!(resp.candidates[1], vec![json!(99)]);
}

#[tokio::test]
async fn rate_limit_429_retries_with_backoff() {
    let server = MockServer::start().await;

    // First two calls return 429, third succeeds.
    Mock::given(method("POST"))
        .and(path("/v1/generate"))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"x": 42}])))
        .mount(&server)
        .await;

    let adapter = StubHttpAdapter::new(&server.uri());
    let wrapped = RateLimitedOracle::new(adapter, 3);
    let resp = wrapped.query(make_ctx()).await.unwrap();
    assert_eq!(resp.candidates.len(), 1);
    assert_eq!(resp.candidates[0], vec![json!(42)]);
}

#[tokio::test]
async fn max_retries_exhausted() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/generate"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;

    let adapter = StubHttpAdapter::new(&server.uri());
    let wrapped = RateLimitedOracle::new(adapter, 2);
    let err = wrapped.query(make_ctx()).await.unwrap_err();
    assert!(err.downcast_ref::<OracleError>().is_some());
}

#[tokio::test]
async fn malformed_json_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_string("this is not json at all"))
        .mount(&server)
        .await;

    let adapter = StubHttpAdapter::new(&server.uri());
    let resp = adapter.query(make_ctx()).await.unwrap();
    assert!(resp.candidates.is_empty());
}

#[tokio::test]
async fn auth_error_401_no_retry() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/generate"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let adapter = StubHttpAdapter::new(&server.uri());
    let wrapped = RateLimitedOracle::new(adapter, 5);
    let err = wrapped.query(make_ctx()).await.unwrap_err();
    assert!(err.to_string().contains("401"));
    // Should not have retried — only 1 request made.
    server.verify().await;
}
