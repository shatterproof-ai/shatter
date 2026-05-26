//! Local subprocess adapter: starts an OpenAI-compatible server, waits for
//! readiness, then delegates to [`CustomHttpAdapter`] for actual queries.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Command;
use tokio::sync::OnceCell;

use shatter_core::config::{CustomAdapterConfig, CustomAuthMode, LlmConfig, LocalAdapterConfig};
use shatter_core::oracle::{OracleContext, OracleResponse, SeedOracle};

use crate::custom_http::CustomHttpAdapter;

pub struct LocalModelAdapter {
    local_cfg: LocalAdapterConfig,
    llm: LlmConfig,
    inner: OnceCell<Arc<CustomHttpAdapter>>,
}

impl LocalModelAdapter {
    pub fn new(llm: LlmConfig) -> anyhow::Result<Self> {
        let local_cfg = llm
            .local
            .clone()
            .ok_or_else(|| anyhow::anyhow!("llm.local config required when adapter = \"local\""))?;

        Ok(Self {
            local_cfg,
            llm,
            inner: OnceCell::new(),
        })
    }

    async fn ensure_ready(&self) -> anyhow::Result<&Arc<CustomHttpAdapter>> {
        self.inner
            .get_or_try_init(|| async {
                self.start_and_wait().await?;

                let custom_cfg = CustomAdapterConfig {
                    url: format!(
                        "http://127.0.0.1:{}/v1/chat/completions",
                        self.local_cfg.port
                    ),
                    headers: std::collections::HashMap::new(),
                    auth: CustomAuthMode::None,
                    request_path: "$.messages[0].content".to_string(),
                    response_path: "$.choices[0].message.content".to_string(),
                };

                let mut llm = self.llm.clone();
                llm.custom = Some(custom_cfg);

                let adapter = CustomHttpAdapter::new(llm)?;
                Ok(Arc::new(adapter))
            })
            .await
    }

    async fn start_and_wait(&self) -> anyhow::Result<()> {
        if self.health_check().await {
            return Ok(());
        }

        let (program, args) = self
            .local_cfg
            .command
            .split_first()
            .ok_or_else(|| anyhow::anyhow!("llm.local.command must not be empty"))?;

        let _child = Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| anyhow::anyhow!("failed to start local model server: {e}"))?;

        let deadline = tokio::time::Instant::now()
            + Duration::from_secs(u64::from(self.local_cfg.startup_timeout_seconds));

        while tokio::time::Instant::now() < deadline {
            if self.health_check().await {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        anyhow::bail!(
            "local model server did not become ready within {}s",
            self.local_cfg.startup_timeout_seconds
        )
    }

    async fn health_check(&self) -> bool {
        let url = format!("http://127.0.0.1:{}/v1/models", self.local_cfg.port);
        reqwest::Client::new()
            .get(&url)
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .is_ok_and(|r| r.status().is_success())
    }
}

#[async_trait]
impl SeedOracle for LocalModelAdapter {
    async fn query(&self, ctx: OracleContext) -> anyhow::Result<OracleResponse> {
        let adapter = self.ensure_ready().await?;
        adapter.query(ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_without_local_config_errors() {
        let llm = LlmConfig::default();
        match LocalModelAdapter::new(llm) {
            Err(e) => assert!(e.to_string().contains("llm.local config required")),
            Ok(_) => panic!("expected error without local config"),
        }
    }

    #[test]
    fn new_with_local_config_succeeds() {
        let llm = LlmConfig {
            local: Some(LocalAdapterConfig {
                command: vec!["echo".into()],
                model: "test".into(),
                port: 11434,
                startup_timeout_seconds: 5,
            }),
            ..LlmConfig::default()
        };
        assert!(LocalModelAdapter::new(llm).is_ok());
    }
}
