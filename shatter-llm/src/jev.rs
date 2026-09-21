//! Jev (TypeSafe "System One") adapter for [`DecisionOracle`] (str-hjrnp.2).
//!
//! API contract as read from docs.typesafe.ai on 2026-09-21:
//! `POST /v1/systemone` with `Authorization: Bearer <key>` and a body
//! `{state, model, questions: {<id>: {type: "choice", instructions,
//! criteria: {<key>: <desc>}}}}`; the response carries
//! `answers.<id>.{choice, probabilities, confidence}` and
//! `usage.{input_tokens, output_tokens}`. One choice question per call.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use shatter_core::decision::{ChoiceRequest, ChoiceResponse, DecisionOracle};

pub const DEFAULT_JEV_URL: &str = "https://api.typesafe.ai/v1/systemone";
pub const DEFAULT_JEV_MODEL: &str = "jev-latest";
const QUESTION_ID: &str = "frontier";

#[derive(Debug, Clone)]
pub struct JevConfig {
    pub url: String,
    pub api_key: String,
    pub model: String,
    pub timeout_seconds: u32,
}

impl JevConfig {
    /// Build from `TYPESAFE_API_KEY`; `None` when it is unset or empty.
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("TYPESAFE_API_KEY").ok()?;
        if api_key.is_empty() {
            return None;
        }
        Some(Self {
            url: DEFAULT_JEV_URL.into(),
            api_key,
            model: DEFAULT_JEV_MODEL.into(),
            timeout_seconds: 10,
        })
    }
}

#[derive(Debug)]
pub struct JevAdapter {
    client: Client,
    config: JevConfig,
}

impl JevAdapter {
    pub fn new(config: JevConfig) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(u64::from(config.timeout_seconds)))
            .build()?;
        Ok(Self { client, config })
    }

    fn body(&self, req: &ChoiceRequest) -> Value {
        let criteria: serde_json::Map<String, Value> = req
            .criteria
            .iter()
            .map(|(k, d)| (k.clone(), Value::String(d.clone())))
            .collect();
        json!({
            "state": req.state,
            "model": self.config.model,
            "questions": {
                QUESTION_ID: {
                    "type": "choice",
                    "instructions": req.instructions,
                    "criteria": criteria,
                }
            }
        })
    }
}

#[derive(Deserialize)]
struct JevAnswer {
    choice: String,
    probabilities: HashMap<String, f64>,
    #[serde(default)]
    confidence: f64,
}

#[derive(Deserialize, Default)]
struct JevUsage {
    #[serde(default)]
    input_tokens: u32,
}

#[derive(Deserialize)]
struct JevResponse {
    answers: HashMap<String, JevAnswer>,
    #[serde(default)]
    usage: Option<JevUsage>,
}

#[async_trait]
impl DecisionOracle for JevAdapter {
    fn name(&self) -> &'static str {
        "jev"
    }

    async fn choose(&self, req: &ChoiceRequest) -> anyhow::Result<ChoiceResponse> {
        req.validate()?;
        let resp = self
            .client
            .post(&self.config.url)
            .bearer_auth(&self.config.api_key)
            .json(&self.body(req))
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!(
                "jev request failed with HTTP {}: {}",
                status.as_u16(),
                text.chars().take(300).collect::<String>()
            );
        }
        let mut parsed: JevResponse = resp.json().await?;
        let answer = parsed
            .answers
            .remove(QUESTION_ID)
            .ok_or_else(|| anyhow::anyhow!("jev response missing answer {QUESTION_ID:?}"))?;
        Ok(ChoiceResponse {
            choice: answer.choice,
            probabilities: answer.probabilities,
            confidence: answer.confidence,
            input_tokens: parsed.usage.unwrap_or_default().input_tokens,
        })
    }
}
