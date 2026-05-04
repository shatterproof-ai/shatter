//! Centralized setup/teardown lifecycle management.
//!
//! Manages four-level setup lifecycle (session → file → function → execution)
//! with per-level timeouts, failure tracking, and context caching. The
//! `SetupManager` tracks which levels have been initialized and supports
//! skipping setup when a prior failure at a higher level makes it pointless.

use std::collections::HashMap;
use std::time::Duration;

use crate::protocol::SetupLevel;

// ---------------------------------------------------------------------------
// Timeout constants
// ---------------------------------------------------------------------------

/// Default timeout for session-level setup/teardown.
pub const DEFAULT_SESSION_TIMEOUT: Duration = Duration::from_secs(120);

/// Default timeout for file-level setup/teardown.
pub const DEFAULT_FILE_TIMEOUT: Duration = Duration::from_secs(30);

/// Default timeout for function-level setup/teardown.
pub const DEFAULT_FUNCTION_TIMEOUT: Duration = Duration::from_secs(15);

/// Default timeout for execution-level setup/teardown.
pub const DEFAULT_EXECUTION_TIMEOUT: Duration = Duration::from_secs(5);

/// Environment variable to override all setup timeouts (value in seconds).
pub const SETUP_TIMEOUT_ENV_VAR: &str = "SHATTER_SETUP_TIMEOUT";

// ---------------------------------------------------------------------------
// SetupTimeouts
// ---------------------------------------------------------------------------

/// Per-level timeout configuration for setup/teardown operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupTimeouts {
    pub session: Duration,
    pub file: Duration,
    pub function: Duration,
    pub execution: Duration,
}

impl Default for SetupTimeouts {
    fn default() -> Self {
        Self {
            session: DEFAULT_SESSION_TIMEOUT,
            file: DEFAULT_FILE_TIMEOUT,
            function: DEFAULT_FUNCTION_TIMEOUT,
            execution: DEFAULT_EXECUTION_TIMEOUT,
        }
    }
}

impl SetupTimeouts {
    /// Create timeouts from environment, falling back to defaults.
    ///
    /// If `SHATTER_SETUP_TIMEOUT` is set to a valid positive integer, all
    /// levels use that value. Invalid or non-positive values are silently ignored.
    pub fn from_env() -> Self {
        // Hazard: process env is shared mutable state across parallel tests.
        // Tests must call `from_raw_value` with an explicit string instead of
        // mutating SETUP_TIMEOUT_ENV_VAR — see `tests` module below.
        Self::from_raw_value(std::env::var(SETUP_TIMEOUT_ENV_VAR).ok().as_deref())
    }

    /// Pure parser used by `from_env`. Decouples the env read from the parsing
    /// logic so tests can exercise every branch without touching the process
    /// environment (which races under `cargo test`'s parallel runner).
    pub fn from_raw_value(raw: Option<&str>) -> Self {
        match raw {
            Some(val) => match val.parse::<u64>() {
                Ok(secs) if secs > 0 => {
                    let d = Duration::from_secs(secs);
                    Self {
                        session: d,
                        file: d,
                        function: d,
                        execution: d,
                    }
                }
                _ => Self::default(),
            },
            None => Self::default(),
        }
    }

    /// Get the timeout for a specific level.
    pub fn timeout_for(&self, level: SetupLevel) -> Duration {
        match level {
            SetupLevel::Session => self.session,
            SetupLevel::File => self.file,
            SetupLevel::Function => self.function,
            SetupLevel::Execution => self.execution,
        }
    }
}

// ---------------------------------------------------------------------------
// SetupFailures
// ---------------------------------------------------------------------------

/// Tracks which setup levels have failed, preventing cascading setup attempts
/// at dependent (inner) levels.
#[derive(Debug, Clone, Default)]
pub struct SetupFailures {
    failures: HashMap<SetupLevel, String>,
}

impl SetupFailures {
    /// Record a failure at the given level with an error description.
    pub fn record(&mut self, level: SetupLevel, error: String) {
        self.failures.insert(level, error);
    }

    /// Clear the failure for a given level (e.g., after successful retry).
    pub fn clear(&mut self, level: SetupLevel) {
        self.failures.remove(&level);
    }

    /// Check if a specific level has failed.
    pub fn has_failed(&self, level: SetupLevel) -> bool {
        self.failures.contains_key(&level)
    }

    /// Get the error message for a failed level.
    pub fn error_for(&self, level: SetupLevel) -> Option<&str> {
        self.failures.get(&level).map(|s| s.as_str())
    }
}

// ---------------------------------------------------------------------------
// SetupError
// ---------------------------------------------------------------------------

/// Errors from setup/teardown operations.
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("setup timed out at {level:?} level after {timeout:?}")]
    Timeout {
        level: SetupLevel,
        timeout: Duration,
    },

    #[error("setup failed at {level:?} level: {message}")]
    Failed { level: SetupLevel, message: String },

    #[error("setup skipped at {level:?} level due to prior failure at {cause_level:?}")]
    Skipped {
        level: SetupLevel,
        cause_level: SetupLevel,
    },
}

// ---------------------------------------------------------------------------
// SetupManager
// ---------------------------------------------------------------------------

/// Context value returned by a frontend's setup command.
pub type SetupContext = serde_json::Value;

/// Centralizes setup lifecycle across all four levels.
///
/// Caches context values returned by setup commands so teardown can reference
/// them, and tracks failures to enable `should_skip` decisions.
#[derive(Debug, Clone)]
pub struct SetupManager {
    /// Cached context per level. Key is (level, scope) where scope is a
    /// distinguishing string (e.g., file path for File level, function name
    /// for Function level). Session level uses an empty scope.
    contexts: HashMap<(SetupLevel, String), SetupContext>,

    /// Tracks which levels have failed.
    pub failures: SetupFailures,

    /// Per-level timeouts.
    pub timeouts: SetupTimeouts,

    /// When true, a setup failure at any level is fatal (returns error).
    /// When false, failures are recorded but exploration continues.
    pub fail_on_error: bool,
}

impl SetupManager {
    /// Create a new SetupManager with the given timeouts and fail_on_error policy.
    pub fn new(timeouts: SetupTimeouts, fail_on_error: bool) -> Self {
        Self {
            contexts: HashMap::new(),
            failures: SetupFailures::default(),
            timeouts,
            fail_on_error,
        }
    }

    /// Create with default timeouts (respecting env var) and fail_on_error = false.
    pub fn from_env() -> Self {
        Self::new(SetupTimeouts::from_env(), false)
    }

    /// Record a successful setup at the given level and scope, caching its context.
    pub fn setup(
        &mut self,
        level: SetupLevel,
        scope: &str,
        context: SetupContext,
    ) -> Result<(), SetupError> {
        if let Some(cause_level) = self.blocking_failure(level) {
            return Err(SetupError::Skipped { level, cause_level });
        }
        self.contexts.insert((level, scope.to_string()), context);
        self.failures.clear(level);
        Ok(())
    }

    /// Record a setup failure at the given level.
    pub fn record_failure(&mut self, level: SetupLevel, error: String) -> Result<(), SetupError> {
        self.failures.record(level, error.clone());
        if self.fail_on_error {
            Err(SetupError::Failed {
                level,
                message: error,
            })
        } else {
            Ok(())
        }
    }

    /// Remove the context for the given level and scope (teardown completed).
    pub fn teardown(&mut self, level: SetupLevel, scope: &str) {
        self.contexts.remove(&(level, scope.to_string()));
    }

    /// Get the cached context for a specific level and scope.
    pub fn get_context(&self, level: SetupLevel, scope: &str) -> Option<&SetupContext> {
        self.contexts.get(&(level, scope.to_string()))
    }

    /// Build a context stack from outermost (session) to innermost, collecting
    /// all active contexts for the given scopes.
    ///
    /// `scopes` should be ordered from session to execution, e.g.:
    /// `["", "src/auth.ts", "validateToken", "exec-0"]`
    pub fn context_stack(&self, scopes: &[(SetupLevel, &str)]) -> Vec<(SetupLevel, &SetupContext)> {
        let level_order = [
            SetupLevel::Session,
            SetupLevel::File,
            SetupLevel::Function,
            SetupLevel::Execution,
        ];

        let mut stack = Vec::new();
        for &level in &level_order {
            if let Some((_, scope)) = scopes.iter().find(|(l, _)| *l == level)
                && let Some(ctx) = self.get_context(level, scope)
            {
                stack.push((level, ctx));
            }
        }
        stack
    }

    /// Check whether setup at the given level should be skipped due to a
    /// prior failure at the same or an outer level.
    pub fn should_skip(&self, level: SetupLevel) -> bool {
        self.blocking_failure_inclusive(level).is_some()
    }

    /// Find the outermost failed level that blocks the given level (inclusive).
    /// Used by `should_skip` — includes the same level.
    fn blocking_failure_inclusive(&self, level: SetupLevel) -> Option<SetupLevel> {
        let hierarchy = [
            SetupLevel::Session,
            SetupLevel::File,
            SetupLevel::Function,
            SetupLevel::Execution,
        ];

        let target_idx = hierarchy.iter().position(|l| *l == level).unwrap_or(0);
        hierarchy[..=target_idx]
            .iter()
            .find(|l| self.failures.has_failed(**l))
            .copied()
    }

    /// Find a failed outer level that prevents setup at the given level.
    /// Excludes the same level so retry is allowed.
    fn blocking_failure(&self, level: SetupLevel) -> Option<SetupLevel> {
        let hierarchy = [
            SetupLevel::Session,
            SetupLevel::File,
            SetupLevel::Function,
            SetupLevel::Execution,
        ];

        let target_idx = hierarchy.iter().position(|l| *l == level).unwrap_or(0);
        hierarchy[..target_idx]
            .iter()
            .find(|l| self.failures.has_failed(**l))
            .copied()
    }

    /// Number of active (cached) contexts across all levels.
    pub fn active_context_count(&self) -> usize {
        self.contexts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_timeouts_match_constants() {
        let t = SetupTimeouts::default();
        assert_eq!(t.session, DEFAULT_SESSION_TIMEOUT);
        assert_eq!(t.file, DEFAULT_FILE_TIMEOUT);
        assert_eq!(t.function, DEFAULT_FUNCTION_TIMEOUT);
        assert_eq!(t.execution, DEFAULT_EXECUTION_TIMEOUT);
    }

    #[test]
    fn timeout_for_returns_correct_level() {
        let t = SetupTimeouts::default();
        assert_eq!(t.timeout_for(SetupLevel::Session), DEFAULT_SESSION_TIMEOUT);
        assert_eq!(t.timeout_for(SetupLevel::File), DEFAULT_FILE_TIMEOUT);
        assert_eq!(
            t.timeout_for(SetupLevel::Function),
            DEFAULT_FUNCTION_TIMEOUT
        );
        assert_eq!(
            t.timeout_for(SetupLevel::Execution),
            DEFAULT_EXECUTION_TIMEOUT
        );
    }

    // The three tests below exercise the env-override parsing via the pure
    // `from_raw_value` entry point rather than mutating SETUP_TIMEOUT_ENV_VAR.
    // The process environment is shared mutable state across parallel tests,
    // so set_var/remove_var here would race with any other test (or any
    // `from_env` caller) running concurrently — see str-k2i3.

    #[test]
    fn env_override_applies_to_all_levels() {
        let t = SetupTimeouts::from_raw_value(Some("42"));

        let expected = Duration::from_secs(42);
        assert_eq!(t.session, expected);
        assert_eq!(t.file, expected);
        assert_eq!(t.function, expected);
        assert_eq!(t.execution, expected);
    }

    #[test]
    fn env_invalid_falls_back_to_defaults() {
        let t = SetupTimeouts::from_raw_value(Some("not-a-number"));
        assert_eq!(t, SetupTimeouts::default());
    }

    #[test]
    fn env_zero_falls_back_to_defaults() {
        let t = SetupTimeouts::from_raw_value(Some("0"));
        assert_eq!(t, SetupTimeouts::default());
    }

    #[test]
    fn env_unset_uses_defaults() {
        let t = SetupTimeouts::from_raw_value(None);
        assert_eq!(t, SetupTimeouts::default());
    }

    #[test]
    fn failure_tracking_record_and_check() {
        let mut f = SetupFailures::default();
        assert!(!f.has_failed(SetupLevel::Session));

        f.record(SetupLevel::Session, "connection refused".into());
        assert!(f.has_failed(SetupLevel::Session));
        assert_eq!(f.error_for(SetupLevel::Session), Some("connection refused"));
        assert!(!f.has_failed(SetupLevel::File));
    }

    #[test]
    fn failure_clear_removes_entry() {
        let mut f = SetupFailures::default();
        f.record(SetupLevel::File, "timeout".into());
        assert!(f.has_failed(SetupLevel::File));

        f.clear(SetupLevel::File);
        assert!(!f.has_failed(SetupLevel::File));
        assert!(f.error_for(SetupLevel::File).is_none());
    }

    #[test]
    fn setup_stores_context() {
        let mut mgr = SetupManager::new(SetupTimeouts::default(), false);
        let ctx = serde_json::json!({"db": "test_db"});
        mgr.setup(SetupLevel::Session, "", ctx.clone()).unwrap();

        assert_eq!(mgr.get_context(SetupLevel::Session, ""), Some(&ctx));
        assert_eq!(mgr.active_context_count(), 1);
    }

    #[test]
    fn teardown_removes_context() {
        let mut mgr = SetupManager::new(SetupTimeouts::default(), false);
        mgr.setup(SetupLevel::Session, "", serde_json::json!({}))
            .unwrap();
        assert_eq!(mgr.active_context_count(), 1);

        mgr.teardown(SetupLevel::Session, "");
        assert!(mgr.get_context(SetupLevel::Session, "").is_none());
        assert_eq!(mgr.active_context_count(), 0);
    }

    #[test]
    fn context_stack_returns_ordered_contexts() {
        let mut mgr = SetupManager::new(SetupTimeouts::default(), false);
        let session_ctx = serde_json::json!({"level": "session"});
        let file_ctx = serde_json::json!({"level": "file"});
        let func_ctx = serde_json::json!({"level": "function"});

        mgr.setup(SetupLevel::Session, "", session_ctx.clone())
            .unwrap();
        mgr.setup(SetupLevel::File, "auth.ts", file_ctx.clone())
            .unwrap();
        mgr.setup(SetupLevel::Function, "validate", func_ctx.clone())
            .unwrap();

        let scopes = [
            (SetupLevel::Session, ""),
            (SetupLevel::File, "auth.ts"),
            (SetupLevel::Function, "validate"),
        ];
        let stack = mgr.context_stack(&scopes);
        assert_eq!(stack.len(), 3);
        assert_eq!(stack[0], (SetupLevel::Session, &session_ctx));
        assert_eq!(stack[1], (SetupLevel::File, &file_ctx));
        assert_eq!(stack[2], (SetupLevel::Function, &func_ctx));
    }

    #[test]
    fn context_stack_skips_missing_levels() {
        let mut mgr = SetupManager::new(SetupTimeouts::default(), false);
        let session_ctx = serde_json::json!({"s": true});
        mgr.setup(SetupLevel::Session, "", session_ctx.clone())
            .unwrap();

        let scopes = [
            (SetupLevel::Session, ""),
            (SetupLevel::File, "missing.ts"),
            (SetupLevel::Function, "fn"),
        ];
        let stack = mgr.context_stack(&scopes);
        assert_eq!(stack.len(), 1);
        assert_eq!(stack[0].0, SetupLevel::Session);
    }

    #[test]
    fn should_skip_when_outer_level_failed() {
        let mut mgr = SetupManager::new(SetupTimeouts::default(), false);
        mgr.record_failure(SetupLevel::Session, "db down".into())
            .unwrap();

        assert!(mgr.should_skip(SetupLevel::Session));
        assert!(mgr.should_skip(SetupLevel::File));
        assert!(mgr.should_skip(SetupLevel::Function));
        assert!(mgr.should_skip(SetupLevel::Execution));
    }

    #[test]
    fn should_skip_only_inner_levels() {
        let mut mgr = SetupManager::new(SetupTimeouts::default(), false);
        mgr.record_failure(SetupLevel::Function, "fn setup err".into())
            .unwrap();

        assert!(!mgr.should_skip(SetupLevel::Session));
        assert!(!mgr.should_skip(SetupLevel::File));
        assert!(mgr.should_skip(SetupLevel::Function));
        assert!(mgr.should_skip(SetupLevel::Execution));
    }

    #[test]
    fn fail_on_error_returns_err() {
        let mut mgr = SetupManager::new(SetupTimeouts::default(), true);
        let result = mgr.record_failure(SetupLevel::Session, "fatal".into());
        assert!(result.is_err());
    }

    #[test]
    fn fail_on_error_false_records_silently() {
        let mut mgr = SetupManager::new(SetupTimeouts::default(), false);
        let result = mgr.record_failure(SetupLevel::Session, "non-fatal".into());
        assert!(result.is_ok());
        assert!(mgr.failures.has_failed(SetupLevel::Session));
    }

    #[test]
    fn setup_blocked_by_outer_failure() {
        let mut mgr = SetupManager::new(SetupTimeouts::default(), false);
        mgr.record_failure(SetupLevel::Session, "session failed".into())
            .unwrap();

        let result = mgr.setup(SetupLevel::File, "auth.ts", serde_json::json!({}));
        assert!(result.is_err());
        match result.unwrap_err() {
            SetupError::Skipped { level, cause_level } => {
                assert_eq!(level, SetupLevel::File);
                assert_eq!(cause_level, SetupLevel::Session);
            }
            other => panic!("expected Skipped, got: {other:?}"),
        }
    }

    #[test]
    fn successful_setup_clears_prior_failure() {
        let mut mgr = SetupManager::new(SetupTimeouts::default(), false);
        mgr.record_failure(SetupLevel::File, "first attempt failed".into())
            .unwrap();
        assert!(mgr.failures.has_failed(SetupLevel::File));

        mgr.setup(SetupLevel::File, "auth.ts", serde_json::json!({}))
            .unwrap();
        assert!(!mgr.failures.has_failed(SetupLevel::File));
    }

    #[test]
    fn multiple_scopes_at_same_level() {
        let mut mgr = SetupManager::new(SetupTimeouts::default(), false);
        let ctx_a = serde_json::json!({"file": "a.ts"});
        let ctx_b = serde_json::json!({"file": "b.ts"});

        mgr.setup(SetupLevel::File, "a.ts", ctx_a.clone()).unwrap();
        mgr.setup(SetupLevel::File, "b.ts", ctx_b.clone()).unwrap();

        assert_eq!(mgr.get_context(SetupLevel::File, "a.ts"), Some(&ctx_a));
        assert_eq!(mgr.get_context(SetupLevel::File, "b.ts"), Some(&ctx_b));
        assert_eq!(mgr.active_context_count(), 2);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_setup_level() -> impl Strategy<Value = SetupLevel> {
        prop_oneof![
            Just(SetupLevel::Session),
            Just(SetupLevel::File),
            Just(SetupLevel::Function),
            Just(SetupLevel::Execution),
        ]
    }

    fn arb_duration_secs() -> impl Strategy<Value = u64> {
        1u64..=3600
    }

    proptest! {
        /// All timeouts in a SetupTimeouts are positive (> 0).
        #[test]
        fn timeouts_always_positive(
            s in arb_duration_secs(),
            f in arb_duration_secs(),
            func in arb_duration_secs(),
            e in arb_duration_secs(),
        ) {
            let t = SetupTimeouts {
                session: Duration::from_secs(s),
                file: Duration::from_secs(f),
                function: Duration::from_secs(func),
                execution: Duration::from_secs(e),
            };
            prop_assert!(t.session > Duration::ZERO);
            prop_assert!(t.file > Duration::ZERO);
            prop_assert!(t.function > Duration::ZERO);
            prop_assert!(t.execution > Duration::ZERO);
        }

        /// timeout_for always returns the correct level's duration.
        #[test]
        fn timeout_for_roundtrip(level in arb_setup_level()) {
            let t = SetupTimeouts::default();
            let expected = match level {
                SetupLevel::Session => t.session,
                SetupLevel::File => t.file,
                SetupLevel::Function => t.function,
                SetupLevel::Execution => t.execution,
            };
            prop_assert_eq!(t.timeout_for(level), expected);
        }

        /// Recording and clearing failures is idempotent: clear always removes.
        #[test]
        fn failure_record_clear_idempotent(level in arb_setup_level()) {
            let mut f = SetupFailures::default();
            f.record(level, "err".into());
            prop_assert!(f.has_failed(level));
            f.clear(level);
            prop_assert!(!f.has_failed(level));
            // Double clear is safe
            f.clear(level);
            prop_assert!(!f.has_failed(level));
        }

        /// should_skip is true for the failed level and all inner levels,
        /// but false for all outer levels.
        #[test]
        fn should_skip_respects_hierarchy(failed_level in arb_setup_level()) {
            let mut mgr = SetupManager::new(SetupTimeouts::default(), false);
            mgr.record_failure(failed_level, "test".into()).unwrap();

            let hierarchy = [
                SetupLevel::Session,
                SetupLevel::File,
                SetupLevel::Function,
                SetupLevel::Execution,
            ];
            let failed_idx = hierarchy.iter().position(|l| *l == failed_level).unwrap();
            for (i, &level) in hierarchy.iter().enumerate() {
                if i < failed_idx {
                    prop_assert!(!mgr.should_skip(level),
                        "level {:?} (idx {}) should NOT be skipped when {:?} (idx {}) failed",
                        level, i, failed_level, failed_idx);
                } else {
                    prop_assert!(mgr.should_skip(level),
                        "level {:?} (idx {}) SHOULD be skipped when {:?} (idx {}) failed",
                        level, i, failed_level, failed_idx);
                }
            }
        }
    }
}
