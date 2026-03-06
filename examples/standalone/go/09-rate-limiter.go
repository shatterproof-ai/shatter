package main

import "fmt"

// Example 9: Rate limiting with token bucket algorithm.
// Tests reasoning about numeric relationships between timestamps, counters, and thresholds.

// BucketState tracks token bucket state.
type BucketState struct {
	Tokens         float64
	LastRefillMs   int64
	PenaltyUntilMs int64
}

// RateLimitResult holds the outcome of a rate limit check.
type RateLimitResult struct {
	Allowed         bool
	Reason          string
	RemainingTokens float64
	NewState        BucketState
}

var priorityCosts = map[string]float64{
	"critical": 1,
	"high":     2,
	"normal":   5,
	"low":      10,
}

// CheckRateLimit — 12 branches: capacity≤0→error, refillRate≤0→error,
// clock skew→error, penalty active→denied, tokens capped at capacity,
// critical+tokens≥1→allowed, critical+<1→denied, high+tokens≥2→allowed,
// normal+tokens≥5→allowed, low+tokens≥10→allowed, insufficient→denied,
// unknown priority→denied.
// Analyzer should detect numeric threshold checks combined with string dispatch.
func CheckRateLimit(
	state BucketState,
	capacity float64,
	refillRate float64,
	nowMs int64,
	priority string,
) (RateLimitResult, error) {
	if capacity <= 0 {
		return RateLimitResult{}, fmt.Errorf("invalid capacity")
	}
	if refillRate <= 0 {
		return RateLimitResult{}, fmt.Errorf("invalid refill rate")
	}
	if nowMs < state.LastRefillMs {
		return RateLimitResult{}, fmt.Errorf("clock skew detected")
	}

	if state.PenaltyUntilMs > nowMs {
		return RateLimitResult{
			Allowed:         false,
			Reason:          "denied: penalty active",
			RemainingTokens: 0,
			NewState:        state,
		}, nil
	}

	elapsedMs := float64(nowMs - state.LastRefillMs)
	refillAmount := (elapsedMs / 1000.0) * refillRate
	tokens := state.Tokens + refillAmount
	if tokens > capacity {
		tokens = capacity
	}

	cost, known := priorityCosts[priority]
	if !known {
		return RateLimitResult{
			Allowed:         false,
			Reason:          "denied: unknown priority",
			RemainingTokens: tokens,
			NewState:        BucketState{Tokens: tokens, LastRefillMs: nowMs, PenaltyUntilMs: state.PenaltyUntilMs},
		}, nil
	}

	if tokens >= cost {
		tokens -= cost
		return RateLimitResult{
			Allowed:         true,
			Reason:          "allowed: " + priority,
			RemainingTokens: tokens,
			NewState:        BucketState{Tokens: tokens, LastRefillMs: nowMs, PenaltyUntilMs: state.PenaltyUntilMs},
		}, nil
	}

	return RateLimitResult{
		Allowed:         false,
		Reason:          "denied: insufficient tokens",
		RemainingTokens: tokens,
		NewState:        BucketState{Tokens: tokens, LastRefillMs: nowMs, PenaltyUntilMs: state.PenaltyUntilMs},
	}, nil
}
