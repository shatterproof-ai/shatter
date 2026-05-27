//! Generic HTTP POST adapter for the [`SeedOracle`] trait.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

use shatter_core::config::{CustomAdapterConfig, CustomAuthMode, LlmConfig};
use shatter_core::oracle::{OracleContext, OracleResponse, SeedOracle};

use crate::prompt::build_prompt;
use crate::parse::parse_response;

pub struct CustomHttpAdapter {
    client: Client,
    config: CustomAdapterConfig,
    llm: LlmConfig,
}

impl CustomHttpAdapter {
    pub fn new(llm: LlmConfig) -> anyhow::Result<Self> {
        let adapter_cfg = llm
            .custom
            .clone()
            .ok_or_else(|| anyhow::anyhow!("llm.custom config required when adapter = \"custom\""))?;

        let client = Client::builder()
            .timeout(Duration::from_secs(u64::from(llm.timeout_seconds)))
            .build()?;

        Ok(Self {
            client,
            config: adapter_cfg,
            llm,
        })
    }

    fn build_request_body(&self, prompt: &str) -> Value {
        set_by_path(
            &self.config.request_path,
            Value::String(prompt.to_string()),
        )
    }

    fn extract_response_text(&self, body: &Value) -> anyhow::Result<String> {
        get_by_path(body, &self.config.response_path)
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "response_path {:?} did not resolve to a string in response body",
                    self.config.response_path
                )
            })
    }
}

#[async_trait]
impl SeedOracle for CustomHttpAdapter {
    async fn query(&self, ctx: OracleContext) -> anyhow::Result<OracleResponse> {
        let prompt = build_prompt(&ctx, self.llm.candidates_per_query);
        let body = self.build_request_body(&prompt);

        let mut req = self.client.post(&self.config.url).json(&body);

        for (k, v) in &self.config.headers {
            req = req.header(k.as_str(), v.as_str());
        }

        match &self.config.auth {
            CustomAuthMode::Bearer(token) => {
                req = req.bearer_auth(token);
            }
            CustomAuthMode::ApiKey { header, key } => {
                req = req.header(header.as_str(), key.as_str());
            }
            CustomAuthMode::None => {}
        }

        let resp = req.send().await?;

        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
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

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("custom adapter HTTP {status}: {text}");
        }

        let resp_body: Value = resp.json().await?;
        let text = self.extract_response_text(&resp_body)?;

        let candidates = parse_response(&text, &ctx.param_types, &ctx.attempted);
        Ok(OracleResponse {
            candidates,
            tokens_used: text.len() as u32,
        })
    }
}

/// Minimal JSONPath-like setter: supports `$.a.b.c` and `$.a[N].b` forms.
fn set_by_path(path: &str, value: Value) -> Value {
    let segments = parse_segments(path);
    build_nested(&segments, value)
}

/// Minimal JSONPath-like getter.
fn get_by_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let segments = parse_segments(path);
    let mut current = root;
    for seg in &segments {
        match seg {
            Segment::Key(k) => {
                current = current.get(k.as_str())?;
            }
            Segment::Index(i) => {
                current = current.get(*i)?;
            }
        }
    }
    Some(current)
}

#[derive(Debug)]
enum Segment {
    Key(String),
    Index(usize),
}

fn parse_segments(path: &str) -> Vec<Segment> {
    let stripped = path.strip_prefix("$.").unwrap_or(path);
    let mut segments = Vec::new();
    for part in stripped.split('.') {
        if let Some(bracket_pos) = part.find('[') {
            let key = &part[..bracket_pos];
            if !key.is_empty() {
                segments.push(Segment::Key(key.to_string()));
            }
            let idx_str = &part[bracket_pos + 1..part.len() - 1];
            if let Ok(idx) = idx_str.parse::<usize>() {
                segments.push(Segment::Index(idx));
            }
        } else {
            segments.push(Segment::Key(part.to_string()));
        }
    }
    segments
}

fn build_nested(segments: &[Segment], value: Value) -> Value {
    match segments.first() {
        None => value,
        Some(Segment::Key(k)) => {
            let inner = build_nested(&segments[1..], value);
            serde_json::json!({ k.clone(): inner })
        }
        Some(Segment::Index(i)) => {
            let inner = build_nested(&segments[1..], value);
            let mut arr = vec![Value::Null; *i + 1];
            arr[*i] = inner;
            Value::Array(arr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_by_path_simple() {
        let v = set_by_path("$.messages[0].content", Value::String("hello".into()));
        assert_eq!(
            v["messages"][0]["content"].as_str().unwrap(),
            "hello"
        );
    }

    #[test]
    fn get_by_path_simple() {
        let v = serde_json::json!({
            "choices": [{"message": {"content": "world"}}]
        });
        let got = get_by_path(&v, "$.choices[0].message.content").unwrap();
        assert_eq!(got.as_str().unwrap(), "world");
    }

    #[test]
    fn roundtrip_set_get() {
        let path = "$.data[0].text";
        let v = set_by_path(path, Value::String("test".into()));
        let got = get_by_path(&v, path).unwrap();
        assert_eq!(got.as_str().unwrap(), "test");
    }

    #[test]
    fn get_by_path_missing_returns_none() {
        let v = serde_json::json!({"a": 1});
        assert!(get_by_path(&v, "$.b.c").is_none());
    }
}
