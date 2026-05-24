//! Generic rate-limit wrapper for [`SeedOracle`] adapters.
//!
//! Adapters signal a rate-limit response by returning
//! `Err(OracleError::RateLimit { .. }.into())`. The wrapper retries with
//! exponential backoff up to `max_retries` before giving up.

use std::time::Duration;

use async_trait::async_trait;
use shatter_core::oracle::{OracleContext, OracleResponse, SeedOracle};

/// Structured error type adapters use to signal rate limiting. Wrapped
/// transparently in `anyhow::Error` so it round-trips through the
/// [`SeedOracle`] trait.
#[derive(Debug, thiserror::Error)]
pub enum OracleError {
    #[error("rate limited (status {status_code}), retry after {retry_after:?}")]
    RateLimit {
        status_code: u16,
        retry_after: Option<Duration>,
    },
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Wrap any [`SeedOracle`] with exponential-backoff retry on
/// [`OracleError::RateLimit`].
pub struct RateLimitedOracle<T: SeedOracle> {
    inner: T,
    max_retries: u32,
}

impl<T: SeedOracle> RateLimitedOracle<T> {
    pub fn new(inner: T, max_retries: u32) -> Self {
        Self { inner, max_retries }
    }
}

fn rate_limit_info(err: &anyhow::Error) -> Option<Option<Duration>> {
    err.downcast_ref::<OracleError>().and_then(|e| match e {
        OracleError::RateLimit { retry_after, .. } => Some(*retry_after),
        OracleError::Other(_) => None,
    })
}

#[async_trait]
impl<T: SeedOracle + Send + Sync> SeedOracle for RateLimitedOracle<T> {
    async fn query(&self, ctx: OracleContext) -> anyhow::Result<OracleResponse> {
        let mut attempt: u32 = 0;
        loop {
            let result = self.inner.query(ctx.clone()).await;
            match result {
                Ok(r) => return Ok(r),
                Err(err) => {
                    let Some(retry_after) = rate_limit_info(&err) else {
                        return Err(err);
                    };
                    if attempt >= self.max_retries {
                        return Err(err);
                    }
                    let backoff = retry_after
                        .unwrap_or_else(|| Duration::from_millis(100u64 << attempt));
                    tokio::time::sleep(backoff).await;
                    attempt += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    use shatter_core::oracle::FailedCondition;

    fn ctx() -> OracleContext {
        OracleContext {
            function_source: String::new(),
            param_types: Vec::new(),
            condition: FailedCondition {
                predicate: "p".to_string(),
                location: "t:1".to_string(),
            },
            attempted: Vec::new(),
        }
    }

    enum Outcome {
        RateLimit,
        Other,
        Ok(OracleResponse),
    }

    struct ScriptedOracle {
        outcomes: Mutex<Vec<Outcome>>,
        calls: AtomicU32,
    }

    #[async_trait]
    impl SeedOracle for ScriptedOracle {
        async fn query(&self, _ctx: OracleContext) -> anyhow::Result<OracleResponse> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut g = self.outcomes.lock().unwrap();
            let next = if g.is_empty() {
                Outcome::Ok(OracleResponse {
                    candidates: Vec::new(),
                    tokens_used: 0,
                })
            } else {
                g.remove(0)
            };
            match next {
                Outcome::Ok(r) => Ok(r),
                Outcome::RateLimit => Err(OracleError::RateLimit {
                    status_code: 429,
                    retry_after: Some(Duration::from_millis(1)),
                }
                .into()),
                Outcome::Other => Err(anyhow::anyhow!("network blew up")),
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn retries_on_rate_limit_then_succeeds() {
        let inner = ScriptedOracle {
            outcomes: Mutex::new(vec![
                Outcome::RateLimit,
                Outcome::RateLimit,
                Outcome::Ok(OracleResponse {
                    candidates: vec![vec![serde_json::json!(42)]],
                    tokens_used: 7,
                }),
            ]),
            calls: AtomicU32::new(0),
        };
        let wrapped = RateLimitedOracle::new(inner, 3);
        let r = wrapped.query(ctx()).await.unwrap();
        assert_eq!(r.tokens_used, 7);
        assert_eq!(wrapped.inner.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_after_max_retries() {
        let inner = ScriptedOracle {
            outcomes: Mutex::new(vec![
                Outcome::RateLimit,
                Outcome::RateLimit,
                Outcome::RateLimit,
                Outcome::RateLimit,
            ]),
            calls: AtomicU32::new(0),
        };
        let wrapped = RateLimitedOracle::new(inner, 2);
        let err = wrapped.query(ctx()).await.unwrap_err();
        assert!(err.downcast_ref::<OracleError>().is_some());
        // 1 initial + 2 retries = 3 attempts total.
        assert_eq!(wrapped.inner.calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn non_rate_limit_errors_bypass_retry() {
        let inner = ScriptedOracle {
            outcomes: Mutex::new(vec![Outcome::Other]),
            calls: AtomicU32::new(0),
        };
        let wrapped = RateLimitedOracle::new(inner, 5);
        let err = wrapped.query(ctx()).await.unwrap_err();
        assert!(err.to_string().contains("network blew up"));
        assert_eq!(wrapped.inner.calls.load(Ordering::SeqCst), 1);
    }
}
