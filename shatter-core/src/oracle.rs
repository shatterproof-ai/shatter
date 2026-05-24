//! LLM seed oracle (str-7vs): trait, request/response types, and slot machinery
//! that lets the synchronous concolic orchestrator drive asynchronous LLM
//! queries without ever calling `.await` on the hot path.
//!
//! The orchestrator wakes up per branch condition, calls
//! [`OracleSlotMap::poll`] with a stable [`ConditionId`], and either receives
//! a ready candidate input vector or `None` (still pending, exhausted, or
//! budget-blocked). Background work runs on a tokio [`tokio::runtime::Runtime`]
//! supplied by the caller; permits are gated by a
//! [`tokio::sync::Semaphore`].
//!
//! ## Budget gates
//!
//! `poll` will not fire a new query when any of the following hold:
//! - `query_count >= config.max_queries_per_function`
//! - `tokens_used  >= config.max_token_budget`
//! - the semaphore has no available permit (`try_acquire` fails)

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Semaphore;

use crate::config::LlmConfig;
use crate::types::ParamInfo;

/// A candidate input vector produced (or attempted) for a target function.
/// Each entry is a JSON value corresponding positionally to a function parameter.
pub type InputVector = Vec<serde_json::Value>;

/// Stable identifier for a branch condition the orchestrator is trying to flip.
/// Slot lifecycle is keyed by this id.
pub type ConditionId = u64;

/// A branch predicate the solver could not satisfy on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedCondition {
    /// Human-readable predicate text (e.g. `"x > 10 && y == \"admin\""`).
    pub predicate: String,
    /// Source location (e.g. `"src/auth.ts:42"`).
    pub location: String,
}

/// Everything the oracle needs to propose new candidate inputs for a single
/// failed condition.
#[derive(Debug, Clone)]
pub struct OracleContext {
    /// Source of the function under test (already trimmed to context window).
    pub function_source: String,
    /// Parameter metadata, in declaration order.
    pub param_types: Vec<ParamInfo>,
    /// The branch condition the explorer is trying to flip.
    pub condition: FailedCondition,
    /// Input vectors already tried for this condition. The oracle should
    /// avoid proposing duplicates.
    pub attempted: Vec<InputVector>,
}

/// Oracle response: a batch of candidate input vectors plus the token cost
/// charged against the global budget.
#[derive(Debug, Clone)]
pub struct OracleResponse {
    pub candidates: Vec<InputVector>,
    pub tokens_used: u32,
}

/// Aggregate telemetry surfaced to the orchestrator/reporter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OracleStats {
    pub queries_fired: u32,
    pub tokens_used: u32,
    pub candidates_accepted: u32,
}

/// Adapter trait implemented by concrete LLM backends (Anthropic, mocks, etc).
///
/// Implementations are `Send + Sync` so the slot map can spawn them onto a
/// shared tokio runtime.
#[async_trait]
pub trait SeedOracle: Send + Sync {
    async fn query(&self, ctx: OracleContext) -> anyhow::Result<OracleResponse>;
}

/// Per-condition slot state.
///
/// Slots transition: `Idle` → `Pending` → `Ready` → (drained) → `Idle` (or
/// `Exhausted` once the per-function budget is spent).
pub enum OracleSlot {
    /// No query in flight, no candidates buffered.
    Idle,
    /// A query is in flight on the background runtime.
    Pending(tokio::task::JoinHandle<anyhow::Result<OracleResponse>>),
    /// Candidates have been returned and are waiting to be drained.
    Ready(VecDeque<InputVector>),
    /// The per-function budget is spent; no further queries will fire for
    /// this condition.
    Exhausted,
}

/// Slot machinery: maps [`ConditionId`] to in-flight or buffered oracle work.
///
/// All public methods are **synchronous** — the orchestrator never `.await`s.
/// Background queries run on the supplied tokio runtime; readiness is detected
/// by polling `JoinHandle::is_finished()`.
pub struct OracleSlotMap {
    oracle: Arc<dyn SeedOracle>,
    slots: HashMap<ConditionId, OracleSlot>,
    config: LlmConfig,
    query_count: u32,
    tokens_used: u32,
    accepted: u32,
    semaphore: Arc<Semaphore>,
    runtime: Arc<tokio::runtime::Runtime>,
}

impl OracleSlotMap {
    /// Construct a slot map bound to the given oracle, config, and tokio runtime.
    pub fn new(
        oracle: Arc<dyn SeedOracle>,
        config: LlmConfig,
        runtime: Arc<tokio::runtime::Runtime>,
    ) -> Self {
        let permits = config.max_concurrent_requests.max(1) as usize;
        Self {
            oracle,
            slots: HashMap::new(),
            config,
            query_count: 0,
            tokens_used: 0,
            accepted: 0,
            semaphore: Arc::new(Semaphore::new(permits)),
            runtime,
        }
    }

    /// Synchronously advance the slot for `id` and try to hand back one
    /// candidate input vector.
    ///
    /// Returns:
    /// - `Some(input)` — a candidate is ready right now.
    /// - `None` — no candidate available: query still pending, budget
    ///   exhausted, or no permit / no slot data available yet.
    ///
    /// Budget gates (do not fire a new query when any holds):
    /// - `query_count >= config.max_queries_per_function`
    /// - `tokens_used >= config.max_token_budget`
    /// - `semaphore.try_acquire()` returns no permit
    pub fn poll(&mut self, id: ConditionId, ctx: OracleContext) -> Option<InputVector> {
        // Advance any existing slot first.
        if let Some(slot) = self.slots.remove(&id) {
            match slot {
                OracleSlot::Idle => {
                    // Fall through to fire a new query below.
                }
                OracleSlot::Pending(handle) => {
                    if handle.is_finished() {
                        // Block on the already-completed task — this is a
                        // bounded, non-blocking operation since the future is
                        // done.
                        match self.runtime.block_on(handle) {
                            Ok(Ok(response)) => {
                                self.tokens_used =
                                    self.tokens_used.saturating_add(response.tokens_used);
                                let mut queue: VecDeque<InputVector> =
                                    response.candidates.into();
                                let first = queue.pop_front();
                                if queue.is_empty() {
                                    self.slots.insert(id, OracleSlot::Idle);
                                } else {
                                    self.slots.insert(id, OracleSlot::Ready(queue));
                                }
                                return first;
                            }
                            Ok(Err(_)) | Err(_) => {
                                // Query failed — drop slot back to Idle so a
                                // future poll can retry (subject to budget).
                                self.slots.insert(id, OracleSlot::Idle);
                                return None;
                            }
                        }
                    } else {
                        // Still in flight — re-insert and report not ready.
                        self.slots.insert(id, OracleSlot::Pending(handle));
                        return None;
                    }
                }
                OracleSlot::Ready(mut queue) => {
                    let next = queue.pop_front();
                    if queue.is_empty() {
                        self.slots.insert(id, OracleSlot::Idle);
                    } else {
                        self.slots.insert(id, OracleSlot::Ready(queue));
                    }
                    return next;
                }
                OracleSlot::Exhausted => {
                    self.slots.insert(id, OracleSlot::Exhausted);
                    return None;
                }
            }
        }

        // Budget gates.
        if self.query_count >= self.config.max_queries_per_function {
            self.slots.insert(id, OracleSlot::Exhausted);
            return None;
        }
        if self.tokens_used >= self.config.max_token_budget {
            self.slots.insert(id, OracleSlot::Exhausted);
            return None;
        }

        // Try to acquire a concurrency permit. We move the owned permit into
        // the spawned task so it is released on drop when the query finishes.
        let permit = match self.semaphore.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => {
                self.slots.insert(id, OracleSlot::Idle);
                return None;
            }
        };

        // Fire a new query.
        let oracle = Arc::clone(&self.oracle);
        let handle = self.runtime.spawn(async move {
            let _permit = permit; // released on drop
            oracle.query(ctx).await
        });
        self.query_count = self.query_count.saturating_add(1);
        self.slots.insert(id, OracleSlot::Pending(handle));
        None
    }

    /// Drop the slot for `id`. Any in-flight query is left to complete on its
    /// own (the JoinHandle is dropped, so its result is discarded).
    pub fn retire(&mut self, id: ConditionId) {
        self.slots.remove(&id);
    }

    /// Record that the orchestrator accepted a candidate (i.e. it produced a
    /// concrete execution result, satisfied the condition, or otherwise
    /// counted as useful).
    pub fn record_accepted(&mut self) {
        self.accepted = self.accepted.saturating_add(1);
    }

    /// Current aggregate telemetry.
    pub fn stats(&self) -> OracleStats {
        OracleStats {
            queries_fired: self.query_count,
            tokens_used: self.tokens_used,
            candidates_accepted: self.accepted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Compile-time check: `OracleSlotMap` is `Send`.
    fn _assert_send<T: Send>() {}
    fn _slot_map_is_send() {
        _assert_send::<OracleSlotMap>();
    }

    struct FixedOracle {
        responses: Mutex<VecDeque<OracleResponse>>,
    }

    #[async_trait]
    impl SeedOracle for FixedOracle {
        async fn query(&self, _ctx: OracleContext) -> anyhow::Result<OracleResponse> {
            let mut g = self.responses.lock().unwrap();
            g.pop_front()
                .ok_or_else(|| anyhow::anyhow!("no more canned responses"))
        }
    }

    fn make_ctx() -> OracleContext {
        OracleContext {
            function_source: String::new(),
            param_types: Vec::new(),
            condition: FailedCondition {
                predicate: "x > 0".to_string(),
                location: "test.rs:1".to_string(),
            },
            attempted: Vec::new(),
        }
    }

    fn make_runtime() -> Arc<tokio::runtime::Runtime> {
        Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        )
    }

    #[test]
    fn llm_config_defaults() {
        let cfg = LlmConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.adapter, "anthropic");
        assert_eq!(cfg.candidates_per_query, 3);
        assert_eq!(cfg.max_queries_per_function, 10);
        assert_eq!(cfg.max_concurrent_requests, 4);
        assert_eq!(cfg.max_token_budget, 50_000);
        assert_eq!(cfg.max_tokens_per_query, 1_024);
        assert!((cfg.temperature - 0.7).abs() < 1e-9);
        assert_eq!(cfg.timeout_seconds, 30);
        assert_eq!(cfg.max_retries, 2);
        assert_eq!(cfg.context_lines, 50);
    }

    #[test]
    fn llm_config_yaml_defaults() {
        // Empty `llm:` key fills every field with its default.
        let yaml = "llm: {}\n";
        #[derive(serde::Deserialize)]
        struct Wrapper {
            llm: LlmConfig,
        }
        let w: Wrapper = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(w.llm, LlmConfig::default());
    }

    #[test]
    fn poll_pending_then_ready() {
        let oracle = Arc::new(FixedOracle {
            responses: Mutex::new(VecDeque::from([OracleResponse {
                candidates: vec![
                    vec![serde_json::json!(1)],
                    vec![serde_json::json!(2)],
                ],
                tokens_used: 42,
            }])),
        });
        let rt = make_runtime();
        let mut map = OracleSlotMap::new(oracle, LlmConfig::default(), rt.clone());

        // First poll fires the query and returns None.
        let first = map.poll(7, make_ctx());
        assert!(first.is_none());
        assert_eq!(map.stats().queries_fired, 1);

        // Spin briefly until the background task completes.
        let mut got = None;
        for _ in 0..200 {
            if let Some(v) = map.poll(7, make_ctx()) {
                got = Some(v);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(got, Some(vec![serde_json::json!(1)]));
        // Drain the remaining buffered candidate.
        let second = map.poll(7, make_ctx());
        assert_eq!(second, Some(vec![serde_json::json!(2)]));
        assert_eq!(map.stats().tokens_used, 42);
    }

    #[test]
    fn budget_gate_blocks_new_queries() {
        let oracle = Arc::new(FixedOracle {
            responses: Mutex::new(VecDeque::new()),
        });
        let cfg = LlmConfig {
            max_queries_per_function: 0,
            ..LlmConfig::default()
        };
        let rt = make_runtime();
        let mut map = OracleSlotMap::new(oracle, cfg, rt);
        assert!(map.poll(1, make_ctx()).is_none());
        assert_eq!(map.stats().queries_fired, 0);
        // The slot is now Exhausted; subsequent polls stay None.
        assert!(map.poll(1, make_ctx()).is_none());
        assert_eq!(map.stats().queries_fired, 0);
    }

    #[test]
    fn retire_drops_slot() {
        let oracle = Arc::new(FixedOracle {
            responses: Mutex::new(VecDeque::from([OracleResponse {
                candidates: vec![vec![serde_json::json!(1)]],
                tokens_used: 1,
            }])),
        });
        let rt = make_runtime();
        let mut map = OracleSlotMap::new(oracle, LlmConfig::default(), rt);
        let _ = map.poll(3, make_ctx());
        map.retire(3);
        assert!(!map.slots.contains_key(&3));
    }

    #[test]
    fn record_accepted_increments() {
        let oracle = Arc::new(FixedOracle {
            responses: Mutex::new(VecDeque::new()),
        });
        let rt = make_runtime();
        let mut map = OracleSlotMap::new(oracle, LlmConfig::default(), rt);
        map.record_accepted();
        map.record_accepted();
        assert_eq!(map.stats().candidates_accepted, 2);
    }
}
