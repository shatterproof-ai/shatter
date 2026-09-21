//! Wiremock tests for the Jev adapter and the replay wrapper (str-hjrnp.2).
//! No live network calls.

use std::sync::Arc;
use std::time::Duration;

use shatter_core::decision::{ChoiceRequest, DecisionOracle};
use shatter_llm::jev::JevConfig;
use shatter_llm::{JevAdapter, ReplayDecisionOracle};
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn req() -> ChoiceRequest {
    ChoiceRequest {
        state: "function f(x) { if (x > 3) {} }".into(),
        instructions: "Which branch next?".into(),
        criteria: vec![
            ("b1".into(), "line 1: x > 3".into()),
            ("b2".into(), "line 1: !(x > 3)".into()),
        ],
    }
}

fn config(server: &MockServer, api_key: &str, timeout_seconds: u32) -> JevConfig {
    JevConfig {
        url: format!("{}/v1/systemone", server.uri()),
        api_key: api_key.into(),
        model: "jev-latest".into(),
        timeout_seconds,
    }
}

fn answer(choice: &str, b1: f64, b2: f64, tokens: u32) -> serde_json::Value {
    serde_json::json!({
        "model": "jev-1.13.0",
        "answers": {
            "frontier": {
                "type": "choice",
                "choice": choice,
                "probabilities": {"b1": b1, "b2": b2},
                "confidence": 0.5
            }
        },
        "usage": {"input_tokens": tokens, "output_tokens": 8}
    })
}

#[tokio::test]
async fn jev_adapter_posts_choice_and_parses_probabilities() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("Authorization", "Bearer test-key"))
        .and(body_partial_json(serde_json::json!({
            "model": "jev-latest",
            "questions": {"frontier": {"type": "choice", "criteria": {"b1": "line 1: x > 3"}}}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer("b2", 0.25, 0.75, 120)))
        .expect(1)
        .mount(&server)
        .await;

    let adapter = JevAdapter::new(config(&server, "test-key", 5)).unwrap();
    let r = adapter.choose(&req()).await.unwrap();
    assert_eq!(r.choice, "b2");
    assert!((r.probabilities["b2"] - 0.75).abs() < 1e-9);
    assert!((r.confidence - 0.5).abs() < 1e-9);
    assert_eq!(r.input_tokens, 120);
}

#[tokio::test]
async fn jev_adapter_surfaces_http_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).set_body_string("slow down"))
        .mount(&server)
        .await;
    let adapter = JevAdapter::new(config(&server, "k", 5)).unwrap();
    let err = adapter.choose(&req()).await.unwrap_err().to_string();
    assert!(err.contains("429"), "got: {err}");
    assert!(err.contains("slow down"), "got: {err}");
}

#[tokio::test]
async fn jev_adapter_times_out() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(answer("b1", 1.0, 0.0, 1))
                .set_delay(Duration::from_secs(3)),
        )
        .mount(&server)
        .await;
    let adapter = JevAdapter::new(config(&server, "k", 1)).unwrap();
    assert!(adapter.choose(&req()).await.is_err());
}

#[tokio::test]
async fn jev_adapter_rejects_invalid_request_before_network() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).respond_with(ResponseTemplate::new(200)).expect(0).mount(&server).await;
    let adapter = JevAdapter::new(config(&server, "k", 5)).unwrap();
    let empty = ChoiceRequest { criteria: vec![], ..req() };
    assert!(adapter.choose(&empty).await.is_err());
}

#[tokio::test]
async fn replay_records_then_serves_without_inner() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer("b1", 0.6, 0.4, 50)))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let inner: Arc<dyn DecisionOracle> =
        Arc::new(JevAdapter::new(config(&server, "k", 5)).unwrap());

    let recording = ReplayDecisionOracle::new(Some(inner), dir.path().to_path_buf());
    let first = recording.choose(&req()).await.unwrap();
    let second_live = recording.choose(&req()).await.unwrap(); // served from cache: expect(1) holds
    assert_eq!(first, second_live);

    let replay = ReplayDecisionOracle::new(None, dir.path().to_path_buf());
    let second = replay.choose(&req()).await.unwrap();
    assert_eq!(first.probabilities, second.probabilities);
    assert_eq!(second.input_tokens, 50);

    // Cache files hold the response only, never the request state.
    let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().collect();
    assert_eq!(entries.len(), 1);
    let body = std::fs::read_to_string(entries[0].path()).unwrap();
    assert!(!body.contains("function f(x)"));

    let miss = ChoiceRequest { instructions: "different".into(), ..req() };
    let err = replay.choose(&miss).await.unwrap_err().to_string();
    assert!(err.contains("replay cache miss"), "got: {err}");
}
