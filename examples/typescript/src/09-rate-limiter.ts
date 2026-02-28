// Example 9: Rate limiting with token bucket algorithm
// Tests shatter's ability to reason about numeric relationships between
// timestamps, counters, and thresholds. Common in API gateways, load
// balancers, and abuse prevention systems.

// Token bucket rate limiter with priority levels and penalty tracking.
//
// EXPECTED BRANCHES (10):
//   1. capacity <= 0                             → throws Error("invalid capacity")
//   2. refillRate <= 0                           → throws Error("invalid refill rate")
//   3. nowMs < lastRefillMs                      → throws Error("clock skew detected")
//   4. penalty active (penaltyUntilMs > nowMs)   → "denied: penalty active"
//   5. after refill, tokens >= capacity (full)   → tokens capped at capacity
//   6. priority === "critical", tokens >= 1      → "allowed: critical"
//   7. priority === "critical", tokens < 1       → "denied: no tokens for critical"
//   8. priority === "high", tokens >= 2          → "allowed: high"
//   9. priority === "normal", tokens >= 5        → "allowed: normal"
//  10. priority === "low", tokens >= 10          → "allowed: low"
//  11. insufficient tokens for priority level    → "denied: insufficient tokens"
//  12. unknown priority                          → "denied: unknown priority"
//
// DIFFICULTY: Hard. The refill calculation involves elapsed time * rate,
// capped at capacity. The solver must find timestamp values that produce
// specific token counts, then combine with priority string matching.

interface BucketState {
    tokens: number;
    lastRefillMs: number;
    penaltyUntilMs: number;
}

type Priority = "critical" | "high" | "normal" | "low";

const PRIORITY_COSTS: Record<Priority, number> = {
    critical: 1,
    high: 2,
    normal: 5,
    low: 10,
};

interface RateLimitResult {
    allowed: boolean;
    reason: string;
    remainingTokens: number;
    newState: BucketState;
}

export function checkRateLimit(
    state: BucketState,
    capacity: number,
    refillRate: number,
    nowMs: number,
    priority: string
): RateLimitResult {
    if (capacity <= 0) {
        throw new Error("invalid capacity");
    }
    if (refillRate <= 0) {
        throw new Error("invalid refill rate");
    }
    if (nowMs < state.lastRefillMs) {
        throw new Error("clock skew detected");
    }

    // Check penalty
    if (state.penaltyUntilMs > nowMs) {
        return {
            allowed: false,
            reason: "denied: penalty active",
            remainingTokens: 0,
            newState: state,
        };
    }

    // Refill tokens based on elapsed time
    const elapsedMs = nowMs - state.lastRefillMs;
    const refillAmount = (elapsedMs / 1000) * refillRate;
    let tokens = Math.min(state.tokens + refillAmount, capacity);

    // Determine cost based on priority
    const cost = PRIORITY_COSTS[priority as Priority];
    if (cost === undefined) {
        return {
            allowed: false,
            reason: "denied: unknown priority",
            remainingTokens: tokens,
            newState: { tokens, lastRefillMs: nowMs, penaltyUntilMs: state.penaltyUntilMs },
        };
    }

    if (tokens >= cost) {
        tokens -= cost;
        return {
            allowed: true,
            reason: `allowed: ${priority}`,
            remainingTokens: tokens,
            newState: { tokens, lastRefillMs: nowMs, penaltyUntilMs: state.penaltyUntilMs },
        };
    }

    return {
        allowed: false,
        reason: "denied: insufficient tokens",
        remainingTokens: tokens,
        newState: { tokens, lastRefillMs: nowMs, penaltyUntilMs: state.penaltyUntilMs },
    };
}
