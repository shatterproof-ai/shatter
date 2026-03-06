// Example 9: Rate limiting with token bucket algorithm.
// Tests reasoning about numeric relationships between timestamps, counters, and thresholds.

struct BucketState {
    tokens: f64,
    last_refill_ms: i64,
    penalty_until_ms: i64,
}

struct RateLimitResult {
    allowed: bool,
    reason: String,
    remaining_tokens: f64,
    new_state: BucketState,
}

const PRIORITY_CRITICAL: f64 = 1.0;
const PRIORITY_HIGH: f64 = 2.0;
const PRIORITY_NORMAL: f64 = 5.0;
const PRIORITY_LOW: f64 = 10.0;

/// check_rate_limit — 12 branches: capacity≤0→error, refill_rate≤0→error,
/// clock skew→error, penalty active→denied, tokens capped at capacity,
/// critical+tokens≥1→allowed, critical+<1→denied, high+tokens≥2→allowed,
/// normal+tokens≥5→allowed, low+tokens≥10→allowed, insufficient→denied,
/// unknown priority→denied.
fn check_rate_limit(
    state: BucketState,
    capacity: f64,
    refill_rate: f64,
    now_ms: i64,
    priority: &str,
) -> Result<RateLimitResult, String> {
    if capacity <= 0.0 {
        return Err("invalid capacity".to_string());
    }
    if refill_rate <= 0.0 {
        return Err("invalid refill rate".to_string());
    }
    if now_ms < state.last_refill_ms {
        return Err("clock skew detected".to_string());
    }

    if state.penalty_until_ms > now_ms {
        return Ok(RateLimitResult {
            allowed: false,
            reason: "denied: penalty active".to_string(),
            remaining_tokens: 0.0,
            new_state: state,
        });
    }

    let elapsed_ms = (now_ms - state.last_refill_ms) as f64;
    let refill_amount = (elapsed_ms / 1000.0) * refill_rate;
    let mut tokens = (state.tokens + refill_amount).min(capacity);

    let cost = match priority {
        "critical" => PRIORITY_CRITICAL,
        "high" => PRIORITY_HIGH,
        "normal" => PRIORITY_NORMAL,
        "low" => PRIORITY_LOW,
        _ => {
            return Ok(RateLimitResult {
                allowed: false,
                reason: "denied: unknown priority".to_string(),
                remaining_tokens: tokens,
                new_state: BucketState {
                    tokens,
                    last_refill_ms: now_ms,
                    penalty_until_ms: state.penalty_until_ms,
                },
            });
        }
    };

    if tokens >= cost {
        tokens -= cost;
        return Ok(RateLimitResult {
            allowed: true,
            reason: format!("allowed: {priority}"),
            remaining_tokens: tokens,
            new_state: BucketState {
                tokens,
                last_refill_ms: now_ms,
                penalty_until_ms: state.penalty_until_ms,
            },
        });
    }

    Ok(RateLimitResult {
        allowed: false,
        reason: "denied: insufficient tokens".to_string(),
        remaining_tokens: tokens,
        new_state: BucketState {
            tokens,
            last_refill_ms: now_ms,
            penalty_until_ms: state.penalty_until_ms,
        },
    })
}

fn main() {
    let state = BucketState {
        tokens: 10.0,
        last_refill_ms: 0,
        penalty_until_ms: 0,
    };
    match check_rate_limit(state, 100.0, 10.0, 1000, "normal") {
        Ok(r) => println!("allowed={}, reason={}, remaining={}", r.allowed, r.reason, r.remaining_tokens),
        Err(e) => println!("Error: {e}"),
    }
}
