//! str-m0ta: Oracle unit tests exercising the slot-map machinery with
//! MockSeedOracle. All tests use the mock — no real LLM API key required.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;

use shatter_core::config::LlmConfig;
use shatter_core::oracle::{
    ConditionId, FailedCondition, InputVector, OracleContext, OracleResponse, OracleSlotMap,
    SeedOracle,
};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn make_ctx(predicate: &str) -> OracleContext {
    OracleContext {
        function_source: "fn foo(x: i32) -> bool { x > 0 }".to_string(),
        param_types: Vec::new(),
        condition: FailedCondition {
            predicate: predicate.to_string(),
            location: "test.rs:1".to_string(),
        },
        attempted: Vec::new(),
    }
}

fn make_runtime() -> Arc<tokio::runtime::Runtime> {
    Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap(),
    )
}

/// Oracle that returns scripted responses from a queue. Thread-safe.
struct ScriptedOracle {
    responses: Mutex<VecDeque<anyhow::Result<OracleResponse>>>,
}

impl ScriptedOracle {
    fn new(responses: Vec<anyhow::Result<OracleResponse>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
        }
    }
}

#[async_trait]
impl SeedOracle for ScriptedOracle {
    async fn query(&self, _ctx: OracleContext) -> anyhow::Result<OracleResponse> {
        let mut g = self.responses.lock().unwrap();
        g.pop_front()
            .unwrap_or_else(|| Err(anyhow::anyhow!("no more canned responses")))
    }
}

/// Oracle that returns a response keyed by the condition's predicate.
///
/// Unlike `ScriptedOracle`, which hands out responses in FIFO order to
/// whichever async query task reaches it first, this oracle deterministically
/// maps each condition (by predicate text) to its own response. This makes
/// tests that fire queries for multiple independent slots insensitive to the
/// order in which tokio schedules those tasks — the arrival race can reorder
/// which task hits the oracle first, but each still gets the response for its
/// own condition.
struct KeyedOracle {
    responses: Mutex<HashMap<String, anyhow::Result<OracleResponse>>>,
}

impl KeyedOracle {
    fn new(responses: Vec<(&str, anyhow::Result<OracleResponse>)>) -> Self {
        Self {
            responses: Mutex::new(
                responses
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            ),
        }
    }
}

#[async_trait]
impl SeedOracle for KeyedOracle {
    async fn query(&self, ctx: OracleContext) -> anyhow::Result<OracleResponse> {
        let mut g = self.responses.lock().unwrap();
        match g.remove(&ctx.condition.predicate) {
            Some(r) => r,
            None => Err(anyhow::anyhow!(
                "no canned response for predicate {:?}",
                ctx.condition.predicate
            )),
        }
    }
}

/// Oracle that delays before responding (for concurrency tests).
struct DelayedOracle {
    delay: Duration,
    response: OracleResponse,
}

#[async_trait]
impl SeedOracle for DelayedOracle {
    async fn query(&self, _ctx: OracleContext) -> anyhow::Result<OracleResponse> {
        tokio::time::sleep(self.delay).await;
        Ok(self.response.clone())
    }
}

/// Oracle that counts invocations (for retry/budget tests).
struct CountingOracle {
    call_count: Arc<Mutex<u32>>,
    response: OracleResponse,
}

#[async_trait]
impl SeedOracle for CountingOracle {
    async fn query(&self, _ctx: OracleContext) -> anyhow::Result<OracleResponse> {
        let mut count = self.call_count.lock().unwrap();
        *count += 1;
        Ok(self.response.clone())
    }
}

/// Poll a slot map until a result or timeout.
fn poll_until_ready(
    map: &mut OracleSlotMap,
    id: ConditionId,
    ctx: OracleContext,
    timeout: Duration,
) -> Option<InputVector> {
    let start = std::time::Instant::now();
    loop {
        if let Some(v) = map.poll(id, ctx.clone()) {
            return Some(v);
        }
        if start.elapsed() > timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

// ---------------------------------------------------------------------------
// Test 1: Successful seeding — oracle returns candidates, slot drains them
// ---------------------------------------------------------------------------

#[test]
fn t01_successful_seeding_drains_all_candidates() {
    let oracle = Arc::new(ScriptedOracle::new(vec![Ok(OracleResponse {
        candidates: vec![vec![json!(1)], vec![json!(2)], vec![json!(3)]],
        tokens_used: 100,
    })]));
    let rt = make_runtime();
    let mut map = OracleSlotMap::new(oracle, LlmConfig::default(), rt);
    let ctx = make_ctx("x > 0");

    // First poll fires query, returns None.
    assert!(map.poll(1, ctx.clone()).is_none());
    assert_eq!(map.stats().queries_fired, 1);

    // Poll until first candidate ready.
    let v1 = poll_until_ready(&mut map, 1, ctx.clone(), Duration::from_secs(5));
    assert_eq!(v1, Some(vec![json!(1)]));

    // Remaining candidates are buffered — immediate.
    assert_eq!(map.poll(1, ctx.clone()), Some(vec![json!(2)]));
    assert_eq!(map.poll(1, ctx.clone()), Some(vec![json!(3)]));
    assert_eq!(map.stats().tokens_used, 100);
    assert_eq!(map.stats().queries_fired, 1);
}

// ---------------------------------------------------------------------------
// Test 2: Empty result — oracle returns no candidates
// ---------------------------------------------------------------------------

#[test]
fn t02_empty_result_returns_none_and_stays_idle() {
    let oracle = Arc::new(ScriptedOracle::new(vec![Ok(OracleResponse {
        candidates: vec![],
        tokens_used: 10,
    })]));
    let rt = make_runtime();
    let mut map = OracleSlotMap::new(oracle, LlmConfig::default(), rt);
    let ctx = make_ctx("x < 0");

    assert!(map.poll(1, ctx.clone()).is_none()); // fires query
    // Wait for completion.
    std::thread::sleep(Duration::from_millis(50));
    // Next poll: query completed with empty candidates → returns None.
    let result = map.poll(1, ctx.clone());
    assert!(result.is_none());
    assert_eq!(map.stats().tokens_used, 10);
    assert_eq!(map.stats().queries_fired, 1);
}

// ---------------------------------------------------------------------------
// Test 3: Always-fail oracle — error path transitions slot back to Idle
// ---------------------------------------------------------------------------

#[test]
fn t03_always_fail_oracle_returns_none_and_allows_retry() {
    let oracle = Arc::new(ScriptedOracle::new(vec![
        Err(anyhow::anyhow!("API timeout")),
        Ok(OracleResponse {
            candidates: vec![vec![json!(42)]],
            tokens_used: 5,
        }),
    ]));
    let rt = make_runtime();
    let mut map = OracleSlotMap::new(oracle, LlmConfig::default(), rt);
    let ctx = make_ctx("x == 42");

    // First query fires and fails.
    assert!(map.poll(1, ctx.clone()).is_none());
    std::thread::sleep(Duration::from_millis(50));
    assert!(map.poll(1, ctx.clone()).is_none()); // error → Idle

    // Second poll fires a new query (retry from Idle).
    assert!(map.poll(1, ctx.clone()).is_none());
    let v = poll_until_ready(&mut map, 1, ctx.clone(), Duration::from_secs(5));
    assert_eq!(v, Some(vec![json!(42)]));
    assert_eq!(map.stats().queries_fired, 2);
}

// ---------------------------------------------------------------------------
// Test 4: Query budget cap — max_queries_per_function blocks new queries
// ---------------------------------------------------------------------------

#[test]
fn t04_query_budget_cap_blocks_after_limit() {
    let call_count = Arc::new(Mutex::new(0u32));
    let oracle = Arc::new(CountingOracle {
        call_count: call_count.clone(),
        response: OracleResponse {
            candidates: vec![vec![json!(1)]],
            tokens_used: 10,
        },
    });
    let cfg = LlmConfig {
        max_queries_per_function: 2,
        ..LlmConfig::default()
    };
    let rt = make_runtime();
    let mut map = OracleSlotMap::new(oracle, cfg, rt);
    let ctx = make_ctx("budget_test");

    // Fire 2 queries (the max).
    map.poll(1, ctx.clone()); // query 1
    poll_until_ready(&mut map, 1, ctx.clone(), Duration::from_secs(5));
    map.poll(1, ctx.clone()); // drain + query 2
    poll_until_ready(&mut map, 1, ctx.clone(), Duration::from_secs(5));

    // Third attempt should be blocked (budget exhausted).
    // Drain remaining if any, then try to fire again.
    while map.poll(1, ctx.clone()).is_some() {}
    // Now try to fire — should be blocked.
    assert!(map.poll(1, ctx.clone()).is_none());
    assert_eq!(map.stats().queries_fired, 2);
    assert_eq!(*call_count.lock().unwrap(), 2);
}

// ---------------------------------------------------------------------------
// Test 5: Token budget cap — max_token_budget blocks after threshold
// ---------------------------------------------------------------------------

#[test]
fn t05_token_budget_cap_blocks_after_threshold() {
    let oracle = Arc::new(ScriptedOracle::new(vec![
        Ok(OracleResponse {
            candidates: vec![vec![json!(1)]],
            tokens_used: 45_000,
        }),
        Ok(OracleResponse {
            candidates: vec![vec![json!(2)]],
            tokens_used: 10_000,
        }),
    ]));
    let cfg = LlmConfig {
        max_token_budget: 50_000,
        ..LlmConfig::default()
    };
    let rt = make_runtime();
    let mut map = OracleSlotMap::new(oracle, cfg, rt);
    let ctx = make_ctx("token_test");

    // First query uses 45k tokens.
    map.poll(1, ctx.clone());
    poll_until_ready(&mut map, 1, ctx.clone(), Duration::from_secs(5));
    assert_eq!(map.stats().tokens_used, 45_000);

    // Second query uses 10k, total would be 55k > 50k budget.
    // Drain first result and fire second.
    while map.poll(1, ctx.clone()).is_some() {}
    map.poll(1, ctx.clone()); // fires query 2
    poll_until_ready(&mut map, 1, ctx.clone(), Duration::from_secs(5));
    // Now tokens_used = 55_000 > budget, next poll blocked.
    while map.poll(1, ctx.clone()).is_some() {}
    assert!(map.poll(1, ctx.clone()).is_none());
    assert_eq!(map.stats().tokens_used, 55_000);
    assert_eq!(map.stats().queries_fired, 2);
}

// ---------------------------------------------------------------------------
// Test 6: Concurrency cap — semaphore exhaustion blocks new queries
// ---------------------------------------------------------------------------

#[test]
fn t06_concurrency_cap_blocks_when_permits_exhausted() {
    let oracle = Arc::new(DelayedOracle {
        delay: Duration::from_millis(200),
        response: OracleResponse {
            candidates: vec![vec![json!(1)]],
            tokens_used: 5,
        },
    });
    let cfg = LlmConfig {
        max_concurrent_requests: 1, // Only 1 permit
        ..LlmConfig::default()
    };
    let rt = make_runtime();
    let mut map = OracleSlotMap::new(oracle, cfg, rt);

    // Fire first query on condition 1 — takes the permit.
    assert!(map.poll(1, make_ctx("cond1")).is_none());
    assert_eq!(map.stats().queries_fired, 1);

    // Try to fire on condition 2 — should fail (no permit).
    assert!(map.poll(2, make_ctx("cond2")).is_none());
    // queries_fired should still be 1 (condition 2 didn't fire).
    assert_eq!(map.stats().queries_fired, 1);

    // Wait for condition 1 to complete (permit released).
    let v = poll_until_ready(&mut map, 1, make_ctx("cond1"), Duration::from_secs(5));
    assert_eq!(v, Some(vec![json!(1)]));

    // Now condition 2 can fire.
    assert!(map.poll(2, make_ctx("cond2")).is_none());
    assert_eq!(map.stats().queries_fired, 2);
}

// ---------------------------------------------------------------------------
// Test 7: Multiple conditions are independent slots
// ---------------------------------------------------------------------------

#[test]
fn t07_multiple_conditions_independent_slots() {
    // Each condition maps to its own response by predicate text, so the
    // assertions hold regardless of the order in which tokio schedules the two
    // independent query tasks. A FIFO oracle would let the second task dequeue
    // the first response under scheduler pressure (see str-5ts6k).
    let oracle = Arc::new(KeyedOracle::new(vec![
        (
            "x > 0",
            Ok(OracleResponse {
                candidates: vec![vec![json!("a")]],
                tokens_used: 10,
            }),
        ),
        (
            "y < 5",
            Ok(OracleResponse {
                candidates: vec![vec![json!("b")]],
                tokens_used: 20,
            }),
        ),
    ]));
    let rt = make_runtime();
    let mut map = OracleSlotMap::new(oracle, LlmConfig::default(), rt);

    // Fire queries for two different conditions.
    map.poll(100, make_ctx("x > 0"));
    map.poll(200, make_ctx("y < 5"));
    assert_eq!(map.stats().queries_fired, 2);

    // Each independent slot resolves to the response for its own condition,
    // regardless of which query task reached the oracle first.
    let v1 = poll_until_ready(&mut map, 100, make_ctx("x > 0"), Duration::from_secs(5));
    let v2 = poll_until_ready(&mut map, 200, make_ctx("y < 5"), Duration::from_secs(5));
    assert_eq!(v1, Some(vec![json!("a")]));
    assert_eq!(v2, Some(vec![json!("b")]));
    assert_eq!(map.stats().tokens_used, 30);
}

// ---------------------------------------------------------------------------
// Test 8: Retire drops in-flight slot without blocking
// ---------------------------------------------------------------------------

#[test]
fn t08_retire_drops_inflight_without_blocking() {
    let oracle = Arc::new(DelayedOracle {
        delay: Duration::from_secs(10), // very slow
        response: OracleResponse {
            candidates: vec![vec![json!(1)]],
            tokens_used: 99,
        },
    });
    let rt = make_runtime();
    let mut map = OracleSlotMap::new(oracle, LlmConfig::default(), rt);

    map.poll(5, make_ctx("slow"));
    assert_eq!(map.stats().queries_fired, 1);

    // Retire immediately — should not block.
    map.retire(5);
    // Polling the same ID starts fresh (new Idle slot).
    // But tokens_used stays 0 since the in-flight was abandoned.
    assert_eq!(map.stats().tokens_used, 0);
}

// ---------------------------------------------------------------------------
// Test 9: Stats accumulate across multiple queries
// ---------------------------------------------------------------------------

#[test]
fn t09_stats_accumulate_across_queries() {
    let oracle = Arc::new(ScriptedOracle::new(vec![
        Ok(OracleResponse {
            candidates: vec![vec![json!(1)]],
            tokens_used: 100,
        }),
        Ok(OracleResponse {
            candidates: vec![vec![json!(2)]],
            tokens_used: 200,
        }),
        Ok(OracleResponse {
            candidates: vec![vec![json!(3)]],
            tokens_used: 300,
        }),
    ]));
    let cfg = LlmConfig {
        max_queries_per_function: 3, // exactly 3 queries allowed
        ..LlmConfig::default()
    };
    let rt = make_runtime();
    let mut map = OracleSlotMap::new(oracle, cfg, rt);
    let ctx = make_ctx("stats");

    // Fire and drain three queries sequentially.
    for _ in 0..3 {
        // poll fires or returns buffered
        let v = poll_until_ready(&mut map, 1, ctx.clone(), Duration::from_secs(5));
        assert!(v.is_some());
    }

    map.record_accepted();
    map.record_accepted();

    let stats = map.stats();
    assert_eq!(stats.queries_fired, 3);
    assert_eq!(stats.tokens_used, 600);
    assert_eq!(stats.candidates_accepted, 2);
}

// ---------------------------------------------------------------------------
// Test 10: OracleBundle creates fresh slot maps with shared config
// ---------------------------------------------------------------------------

#[test]
fn t10_oracle_bundle_creates_independent_slot_maps() {
    use shatter_core::oracle::OracleBundle;

    let oracle = Arc::new(ScriptedOracle::new(vec![
        Ok(OracleResponse {
            candidates: vec![vec![json!(1)]],
            tokens_used: 10,
        }),
        Ok(OracleResponse {
            candidates: vec![vec![json!(2)]],
            tokens_used: 20,
        }),
    ]));
    let rt = make_runtime();
    let bundle = OracleBundle {
        oracle,
        config: LlmConfig::default(),
        runtime: rt,
    };

    let mut map1 = bundle.build_slot_map();
    let mut map2 = bundle.build_slot_map();

    // Each map tracks its own stats independently.
    map1.poll(1, make_ctx("a"));
    poll_until_ready(&mut map1, 1, make_ctx("a"), Duration::from_secs(5));
    map2.poll(1, make_ctx("b"));
    poll_until_ready(&mut map2, 1, make_ctx("b"), Duration::from_secs(5));

    assert_eq!(map1.stats().queries_fired, 1);
    assert_eq!(map2.stats().queries_fired, 1);
    assert_eq!(map1.stats().tokens_used, 10);
    assert_eq!(map2.stats().tokens_used, 20);
}
