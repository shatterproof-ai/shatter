//! Mock value space selection and live-first fallback logic.
//!
//! When Shatter encounters an external dependency during exploration, it must
//! decide how to provide return values. [`MockValueSpace`] captures the
//! available strategies; [`LiveFirstState`] implements a state machine that
//! attempts live calls first and falls back to synthetic mocks on connection
//! failure.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// MockValueSpace — strategy for generating mock return values
// ---------------------------------------------------------------------------

/// Strategy for selecting mock return values during exploration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum MockValueSpace {
    /// Try the real service first; fall back to synthetic mocks on connection
    /// failure. This is the default for network/database dependencies.
    LiveFirst,
    /// Use a fixed set of user-provided return values (round-robin).
    FixedSet { values: Vec<serde_json::Value> },
    /// Engine generates values autonomously based on type information.
    Autonomous,
    /// Replay a recorded seed corpus (e.g. from a prior live session).
    Seeded { seed_file: String },
}

// ---------------------------------------------------------------------------
// Live call outcome classification
// ---------------------------------------------------------------------------

/// Coarse classification of a live call attempt.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum LiveCallOutcome {
    /// Call succeeded — the return value is usable.
    Success,
    /// Infrastructure-level failure — the service is unreachable.
    ConnectionFailure { kind: ConnectionFailureKind },
    /// Application-level error (e.g. HTTP 4xx/5xx) — service is reachable but
    /// returned an error. Still useful as an observed behavior.
    AppError { message: String },
}

/// Subcategories of connection failure, used for diagnostics and to decide
/// whether retries are worthwhile.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionFailureKind {
    ConnectionRefused,
    DnsFailure,
    AuthError,
    Timeout,
    Other,
}

// ---------------------------------------------------------------------------
// Error heuristic patterns — named constants for substring matching
// ---------------------------------------------------------------------------

/// Patterns indicating a refused TCP connection.
pub const CONN_REFUSED_PATTERNS: &[&str] = &["ECONNREFUSED", "connection refused", "Connection refused"];

/// Patterns indicating a DNS resolution failure.
pub const DNS_FAILURE_PATTERNS: &[&str] = &[
    "ENOTFOUND",
    "EAI_AGAIN",
    "dns resolution",
    "DNS resolution",
    "getaddrinfo",
    "no such host",
];

/// Patterns indicating an authentication/authorization failure.
pub const AUTH_ERROR_PATTERNS: &[&str] = &[
    "EAUTH",
    "authentication failed",
    "unauthorized",
    "403 Forbidden",
    "401 Unauthorized",
    "invalid credentials",
];

/// Patterns indicating a timeout.
pub const TIMEOUT_PATTERNS: &[&str] = &[
    "ETIMEDOUT",
    "ESOCKETTIMEDOUT",
    "ETIME",
    "timed out",
    "timeout",
    "deadline exceeded",
];

/// HTTP status codes that indicate application-level errors (not infra).
pub const APP_ERROR_STATUS_CODES: &[&str] = &["400 ", "404 ", "409 ", "422 ", "500 ", "502 ", "503 "];

// ---------------------------------------------------------------------------
// Classification functions
// ---------------------------------------------------------------------------

/// Classify an error message as a specific connection failure kind.
///
/// Returns `None` if the message doesn't match any known connection failure
/// pattern (it may be an application error or unknown failure).
pub fn classify_connection_failure(error_msg: &str) -> Option<ConnectionFailureKind> {
    if CONN_REFUSED_PATTERNS.iter().any(|p| error_msg.contains(p)) {
        return Some(ConnectionFailureKind::ConnectionRefused);
    }
    if DNS_FAILURE_PATTERNS.iter().any(|p| error_msg.contains(p)) {
        return Some(ConnectionFailureKind::DnsFailure);
    }
    if AUTH_ERROR_PATTERNS.iter().any(|p| error_msg.contains(p)) {
        return Some(ConnectionFailureKind::AuthError);
    }
    if TIMEOUT_PATTERNS.iter().any(|p| error_msg.contains(p)) {
        return Some(ConnectionFailureKind::Timeout);
    }
    None
}

/// Classify the result of a live call attempt.
///
/// - `Ok(value)` → [`LiveCallOutcome::Success`]
/// - `Err(msg)` where msg matches a connection pattern → [`LiveCallOutcome::ConnectionFailure`]
/// - `Err(msg)` otherwise → [`LiveCallOutcome::AppError`]
pub fn classify_live_call(result: &Result<serde_json::Value, String>) -> LiveCallOutcome {
    match result {
        Ok(_) => LiveCallOutcome::Success,
        Err(msg) => match classify_connection_failure(msg) {
            Some(kind) => LiveCallOutcome::ConnectionFailure { kind },
            None => LiveCallOutcome::AppError {
                message: msg.clone(),
            },
        },
    }
}

// ---------------------------------------------------------------------------
// LiveFirstState — state machine for live-first fallback
// ---------------------------------------------------------------------------

/// State machine tracking whether a live service is available.
///
/// Transitions:
/// - `Untried` + `Success`           → `Available`
/// - `Untried` + `ConnectionFailure` → `Unavailable`
/// - `Untried` + `AppError`          → `Available` (service is reachable)
/// - `Available` + `ConnectionFailure` → `Unavailable`
/// - `Available` + `Success`/`AppError` → `Available`
/// - `Unavailable` is terminal (no retries within a single exploration run)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveFirstState {
    /// Haven't tried calling the service yet.
    #[default]
    Untried,
    /// At least one successful contact — keep using live calls.
    Available,
    /// Connection failure observed — fall back to synthetic mocks for the
    /// remainder of this exploration run.
    Unavailable,
}

impl LiveFirstState {
    /// Advance the state machine based on a call outcome.
    pub fn transition(self, outcome: &LiveCallOutcome) -> Self {
        match (self, outcome) {
            // Terminal state — no recovery within a run.
            (Self::Unavailable, _) => Self::Unavailable,

            // Connection failure from any non-terminal state → unavailable.
            (_, LiveCallOutcome::ConnectionFailure { .. }) => Self::Unavailable,

            // Success or app error from untried/available → available.
            (Self::Untried | Self::Available, LiveCallOutcome::Success | LiveCallOutcome::AppError { .. }) => {
                Self::Available
            }
        }
    }

    /// Whether we should attempt a live call in this state.
    pub fn should_try_live(self) -> bool {
        matches!(self, Self::Untried | Self::Available)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- Serde roundtrip tests --

    #[test]
    fn mock_value_space_serde_roundtrip() {
        let variants = vec![
            MockValueSpace::LiveFirst,
            MockValueSpace::FixedSet {
                values: vec![json!(1), json!("hello"), json!(null)],
            },
            MockValueSpace::Autonomous,
            MockValueSpace::Seeded {
                seed_file: "seeds/api_responses.jsonl".into(),
            },
        ];

        for variant in &variants {
            let json = serde_json::to_string(variant).unwrap();
            let back: MockValueSpace = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, variant, "roundtrip failed for {json}");
        }
    }

    #[test]
    fn live_call_outcome_serde_roundtrip() {
        let variants = vec![
            LiveCallOutcome::Success,
            LiveCallOutcome::ConnectionFailure {
                kind: ConnectionFailureKind::ConnectionRefused,
            },
            LiveCallOutcome::ConnectionFailure {
                kind: ConnectionFailureKind::DnsFailure,
            },
            LiveCallOutcome::ConnectionFailure {
                kind: ConnectionFailureKind::AuthError,
            },
            LiveCallOutcome::ConnectionFailure {
                kind: ConnectionFailureKind::Timeout,
            },
            LiveCallOutcome::ConnectionFailure {
                kind: ConnectionFailureKind::Other,
            },
            LiveCallOutcome::AppError {
                message: "404 Not Found".into(),
            },
        ];

        for variant in &variants {
            let json = serde_json::to_string(variant).unwrap();
            let back: LiveCallOutcome = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, variant, "roundtrip failed for {json}");
        }
    }

    #[test]
    fn live_first_state_serde_roundtrip() {
        for state in &[
            LiveFirstState::Untried,
            LiveFirstState::Available,
            LiveFirstState::Unavailable,
        ] {
            let json = serde_json::to_string(state).unwrap();
            let back: LiveFirstState = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, state);
        }
    }

    // -- Classification tests --

    #[test]
    fn classify_connection_refused() {
        assert_eq!(
            classify_connection_failure("Error: connect ECONNREFUSED 127.0.0.1:5432"),
            Some(ConnectionFailureKind::ConnectionRefused)
        );
    }

    #[test]
    fn classify_dns_failure() {
        assert_eq!(
            classify_connection_failure("getaddrinfo ENOTFOUND api.example.com"),
            Some(ConnectionFailureKind::DnsFailure)
        );
    }

    #[test]
    fn classify_auth_error() {
        assert_eq!(
            classify_connection_failure("401 Unauthorized"),
            Some(ConnectionFailureKind::AuthError)
        );
    }

    #[test]
    fn classify_timeout() {
        assert_eq!(
            classify_connection_failure("request timed out after 30s"),
            Some(ConnectionFailureKind::Timeout)
        );
    }

    #[test]
    fn classify_unknown_error_returns_none() {
        assert_eq!(
            classify_connection_failure("something completely different"),
            None
        );
    }

    #[test]
    fn classify_live_call_success() {
        let result: Result<serde_json::Value, String> = Ok(json!({"id": 1}));
        assert_eq!(classify_live_call(&result), LiveCallOutcome::Success);
    }

    #[test]
    fn classify_live_call_connection_failure() {
        let result: Result<serde_json::Value, String> =
            Err("connect ECONNREFUSED 127.0.0.1:3000".into());
        assert!(matches!(
            classify_live_call(&result),
            LiveCallOutcome::ConnectionFailure {
                kind: ConnectionFailureKind::ConnectionRefused
            }
        ));
    }

    #[test]
    fn classify_live_call_app_error() {
        let result: Result<serde_json::Value, String> = Err("validation failed: name required".into());
        assert!(matches!(
            classify_live_call(&result),
            LiveCallOutcome::AppError { .. }
        ));
    }

    // -- State machine tests --

    #[test]
    fn state_machine_untried_to_available_on_success() {
        let state = LiveFirstState::Untried;
        assert_eq!(
            state.transition(&LiveCallOutcome::Success),
            LiveFirstState::Available
        );
    }

    #[test]
    fn state_machine_untried_to_unavailable_on_conn_failure() {
        let state = LiveFirstState::Untried;
        assert_eq!(
            state.transition(&LiveCallOutcome::ConnectionFailure {
                kind: ConnectionFailureKind::ConnectionRefused
            }),
            LiveFirstState::Unavailable
        );
    }

    #[test]
    fn state_machine_untried_to_available_on_app_error() {
        let state = LiveFirstState::Untried;
        assert_eq!(
            state.transition(&LiveCallOutcome::AppError {
                message: "500 Internal Server Error".into()
            }),
            LiveFirstState::Available
        );
    }

    #[test]
    fn state_machine_available_stays_on_success() {
        let state = LiveFirstState::Available;
        assert_eq!(
            state.transition(&LiveCallOutcome::Success),
            LiveFirstState::Available
        );
    }

    #[test]
    fn state_machine_available_to_unavailable_on_conn_failure() {
        let state = LiveFirstState::Available;
        assert_eq!(
            state.transition(&LiveCallOutcome::ConnectionFailure {
                kind: ConnectionFailureKind::Timeout
            }),
            LiveFirstState::Unavailable
        );
    }

    #[test]
    fn state_machine_unavailable_is_terminal() {
        let state = LiveFirstState::Unavailable;
        assert_eq!(
            state.transition(&LiveCallOutcome::Success),
            LiveFirstState::Unavailable
        );
        assert_eq!(
            state.transition(&LiveCallOutcome::ConnectionFailure {
                kind: ConnectionFailureKind::DnsFailure
            }),
            LiveFirstState::Unavailable
        );
        assert_eq!(
            state.transition(&LiveCallOutcome::AppError {
                message: "ok".into()
            }),
            LiveFirstState::Unavailable
        );
    }

    #[test]
    fn should_try_live_reflects_state() {
        assert!(LiveFirstState::Untried.should_try_live());
        assert!(LiveFirstState::Available.should_try_live());
        assert!(!LiveFirstState::Unavailable.should_try_live());
    }

    #[test]
    fn default_state_is_untried() {
        assert_eq!(LiveFirstState::default(), LiveFirstState::Untried);
    }

    // -- Proptest --

    mod proptests {
        use super::super::*;
        use proptest::prelude::*;
        use serde_json::json;

        fn arb_mock_value_space() -> impl Strategy<Value = MockValueSpace> {
            prop_oneof![
                Just(MockValueSpace::LiveFirst),
                prop::collection::vec(
                    prop_oneof![
                        Just(json!(null)),
                        Just(json!(42)),
                        Just(json!("test")),
                        Just(json!(true)),
                    ],
                    0..5,
                )
                .prop_map(|values| MockValueSpace::FixedSet { values }),
                Just(MockValueSpace::Autonomous),
                "[a-z/_]{1,30}".prop_map(|seed_file| MockValueSpace::Seeded { seed_file }),
            ]
        }

        fn arb_connection_failure_kind() -> impl Strategy<Value = ConnectionFailureKind> {
            prop_oneof![
                Just(ConnectionFailureKind::ConnectionRefused),
                Just(ConnectionFailureKind::DnsFailure),
                Just(ConnectionFailureKind::AuthError),
                Just(ConnectionFailureKind::Timeout),
                Just(ConnectionFailureKind::Other),
            ]
        }

        fn arb_live_call_outcome() -> impl Strategy<Value = LiveCallOutcome> {
            prop_oneof![
                Just(LiveCallOutcome::Success),
                arb_connection_failure_kind()
                    .prop_map(|kind| LiveCallOutcome::ConnectionFailure { kind }),
                ".{0,50}".prop_map(|message| LiveCallOutcome::AppError { message }),
            ]
        }

        fn arb_live_first_state() -> impl Strategy<Value = LiveFirstState> {
            prop_oneof![
                Just(LiveFirstState::Untried),
                Just(LiveFirstState::Available),
                Just(LiveFirstState::Unavailable),
            ]
        }

        proptest! {
            #[test]
            fn mock_value_space_roundtrip(mvs in arb_mock_value_space()) {
                let json = serde_json::to_string(&mvs).unwrap();
                let back: MockValueSpace = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(&back, &mvs);
            }

            #[test]
            fn live_call_outcome_roundtrip(outcome in arb_live_call_outcome()) {
                let json = serde_json::to_string(&outcome).unwrap();
                let back: LiveCallOutcome = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(&back, &outcome);
            }

            #[test]
            fn live_first_state_roundtrip(state in arb_live_first_state()) {
                let json = serde_json::to_string(&state).unwrap();
                let back: LiveFirstState = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(&back, &state);
            }

            // Unavailable is terminal: no outcome can leave it.
            #[test]
            fn unavailable_is_terminal(outcome in arb_live_call_outcome()) {
                prop_assert_eq!(
                    LiveFirstState::Unavailable.transition(&outcome),
                    LiveFirstState::Unavailable
                );
            }

            // Connection failure always leads to Unavailable (from any non-terminal state).
            #[test]
            fn conn_failure_always_unavailable(
                state in arb_live_first_state(),
                kind in arb_connection_failure_kind(),
            ) {
                let next = state.transition(&LiveCallOutcome::ConnectionFailure { kind });
                prop_assert_eq!(next, LiveFirstState::Unavailable);
            }

            // Success from a non-Unavailable state always leads to Available.
            #[test]
            fn success_leads_to_available(state in arb_live_first_state()) {
                if state != LiveFirstState::Unavailable {
                    prop_assert_eq!(
                        state.transition(&LiveCallOutcome::Success),
                        LiveFirstState::Available
                    );
                }
            }

            // should_try_live is false iff state is Unavailable.
            #[test]
            fn should_try_live_iff_not_unavailable(state in arb_live_first_state()) {
                prop_assert_eq!(
                    state.should_try_live(),
                    state != LiveFirstState::Unavailable
                );
            }

            // classify_live_call(Ok(_)) is always Success.
            #[test]
            fn classify_ok_is_success(_v in prop_oneof![Just(json!(1)), Just(json!("x"))]) {
                let outcome = classify_live_call(&Ok(_v));
                prop_assert_eq!(outcome, LiveCallOutcome::Success);
            }
        }
    }
}
