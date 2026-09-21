//! Record/replay wrapper for any [`DecisionOracle`] (str-hjrnp.2): caches
//! responses on disk keyed by request fingerprint so benchmark runs are
//! reproducible and can run offline. Cache files hold only the
//! [`ChoiceResponse`], never the request state.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use shatter_core::decision::{ChoiceRequest, ChoiceResponse, DecisionOracle, request_fingerprint};

#[derive(Debug)]
pub struct ReplayDecisionOracle {
    inner: Option<Arc<dyn DecisionOracle>>,
    cache_dir: PathBuf,
}

impl ReplayDecisionOracle {
    /// `inner = None` means replay-only: a cache miss is an error.
    pub fn new(inner: Option<Arc<dyn DecisionOracle>>, cache_dir: PathBuf) -> Self {
        Self { inner, cache_dir }
    }

    pub fn cache_dir(&self) -> &PathBuf {
        &self.cache_dir
    }

    fn path_for(&self, req: &ChoiceRequest) -> PathBuf {
        self.cache_dir
            .join(format!("{}.json", request_fingerprint(req)))
    }
}

#[async_trait]
impl DecisionOracle for ReplayDecisionOracle {
    fn name(&self) -> &'static str {
        "replay"
    }

    async fn choose(&self, req: &ChoiceRequest) -> anyhow::Result<ChoiceResponse> {
        let path = self.path_for(req);
        if let Ok(bytes) = tokio::fs::read(&path).await {
            match serde_json::from_slice::<ChoiceResponse>(&bytes) {
                Ok(resp) => return Ok(resp),
                Err(e) => log::warn!(
                    "replay cache entry {} is unreadable ({e}); treating as a miss",
                    path.display()
                ),
            }
        }
        let inner = self.inner.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "replay cache miss for {} and no live oracle configured",
                path.display()
            )
        })?;
        let resp = inner.choose(req).await?;
        tokio::fs::create_dir_all(&self.cache_dir).await?;
        // Write-then-rename so an interrupted run never leaves a truncated
        // entry that would poison every later replay of this request.
        let tmp = path.with_extension(format!("json.tmp-{}", std::process::id()));
        tokio::fs::write(&tmp, serde_json::to_vec_pretty(&resp)?).await?;
        tokio::fs::rename(&tmp, &path).await?;
        Ok(resp)
    }
}
