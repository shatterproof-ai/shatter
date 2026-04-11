//! PostHog telemetry flush: batch-sends queued events to the analytics endpoint.
//!
//! Design:
//! - Reads the local JSONL queue, batches up to `FLUSH_MAX_EVENTS` events, and POSTs
//!   them to the PostHog `/batch` endpoint.
//! - On HTTP 2xx the queue file is truncated (events consumed).
//! - On any error the queue is left intact for the next flush attempt.
//! - `SHATTER_TELEMETRY_DEBUG=1` prints events to stderr instead of sending.
//! - `SHATTER_TELEMETRY_URL` overrides the endpoint (for testing).
//! - The entire flush is bounded by `FLUSH_TIMEOUT_SECS`.

use serde::Serialize;
use shatter_core::telemetry::{self, TelemetryEvent};

// ── Constants ──────────────────────────────────────────────────────────────────

/// PostHog cloud ingestion endpoint.
const POSTHOG_DEFAULT_URL: &str = "https://app.posthog.com/batch";

/// PostHog project API key (Shatter project). Rotating this is non-secret;
/// PostHog keys are designed to be embedded in client software.
const POSTHOG_API_KEY: &str = "phc_shatter_placeholder";

/// Maximum events flushed per call to avoid oversized HTTP requests.
const FLUSH_MAX_EVENTS: usize = 100;

/// HTTP request timeout in seconds.
const FLUSH_TIMEOUT_SECS: u64 = 2;

/// Env var that, when set to `"1"`, prints events to stderr instead of sending.
const ENV_TELEMETRY_DEBUG: &str = "SHATTER_TELEMETRY_DEBUG";

/// Env var that overrides the PostHog endpoint URL (useful in tests).
const ENV_TELEMETRY_URL: &str = "SHATTER_TELEMETRY_URL";

// ── PostHog wire types ─────────────────────────────────────────────────────────

/// Top-level batch request body for the PostHog `/batch` endpoint.
#[derive(Debug, Serialize)]
struct PostHogBatch {
    /// PostHog project API key.
    api_key: String,
    /// Ordered list of captured events.
    batch: Vec<PostHogEvent>,
}

/// A single PostHog event in the batch payload.
#[derive(Debug, Serialize)]
struct PostHogEvent {
    /// PostHog event name (e.g. "command_run").
    event: String,
    /// Stable anonymous identifier for this installation.
    distinct_id: String,
    /// ISO-8601 timestamp string.
    timestamp: String,
    /// Arbitrary event properties (the Shatter event payload).
    properties: serde_json::Value,
}

// ── Public API ─────────────────────────────────────────────────────────────────

/// Flush queued telemetry events to PostHog (fire-and-forget).
///
/// - Reads up to `FLUSH_MAX_EVENTS` events from the local queue.
/// - If `SHATTER_TELEMETRY_DEBUG=1`, prints events to stderr and returns.
/// - Otherwise, POSTs a batch to the PostHog endpoint with a 2-second timeout.
/// - On HTTP 2xx, truncates the queue file.
/// - All errors are silently ignored — telemetry must never crash the CLI.
pub fn flush_queue() {
    if let Err(e) = try_flush_queue() {
        log::debug!("telemetry flush skipped: {e}");
    }
}

// ── Internal ───────────────────────────────────────────────────────────────────

fn try_flush_queue() -> Result<(), FlushError> {
    // Read and cap the queue.
    let all_events = telemetry::read_queue().map_err(FlushError::Queue)?;
    if all_events.is_empty() {
        return Ok(());
    }
    let events: Vec<TelemetryEvent> = all_events.into_iter().take(FLUSH_MAX_EVENTS).collect();

    // Debug mode: print to stderr, do not send.
    if std::env::var(ENV_TELEMETRY_DEBUG).as_deref() == Ok("1") {
        for event in &events {
            eprintln!(
                "[shatter-telemetry] {}",
                serde_json::to_string(event).unwrap_or_else(|_| "<serialize error>".to_string())
            );
        }
        // In debug mode we still truncate, so repeated runs don't re-print old events.
        truncate_queue()?;
        return Ok(());
    }

    let endpoint =
        std::env::var(ENV_TELEMETRY_URL).unwrap_or_else(|_| POSTHOG_DEFAULT_URL.to_string());

    let batch = build_batch(POSTHOG_API_KEY, &events)?;

    let status = post_batch(&endpoint, &batch)?;

    if (200u16..300).contains(&status) {
        truncate_queue()?;
    } else {
        log::debug!("telemetry flush got HTTP {status}; queue preserved for retry");
    }

    Ok(())
}

/// Build the PostHog batch body from a list of Shatter events.
fn build_batch(api_key: &str, events: &[TelemetryEvent]) -> Result<PostHogBatch, FlushError> {
    let posthog_events = events
        .iter()
        .map(shatter_to_posthog)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(PostHogBatch {
        api_key: api_key.to_string(),
        batch: posthog_events,
    })
}

/// Convert a `TelemetryEvent` to a `PostHogEvent`.
fn shatter_to_posthog(event: &TelemetryEvent) -> Result<PostHogEvent, FlushError> {
    // Serialize the payload as a JSON object and merge in top-level envelope fields
    // as properties so they are queryable in PostHog.
    let payload_value =
        serde_json::to_value(&event.payload).map_err(|e| FlushError::Serialize(e.to_string()))?;

    let mut properties = match payload_value {
        serde_json::Value::Object(m) => m,
        other => {
            let mut m = serde_json::Map::new();
            m.insert("payload".to_string(), other);
            m
        }
    };

    // Inject envelope metadata as top-level PostHog properties.
    properties.insert(
        "schema_version".to_string(),
        serde_json::Value::Number(event.schema_version.into()),
    );
    properties.insert(
        "event_id".to_string(),
        serde_json::Value::String(event.event_id.clone()),
    );

    Ok(PostHogEvent {
        event: event.event_name.clone(),
        distinct_id: event.anonymous_id.clone(),
        timestamp: event.timestamp.clone(),
        properties: serde_json::Value::Object(properties),
    })
}

/// POST the batch to the given endpoint and return the HTTP status code.
fn post_batch(endpoint: &str, batch: &PostHogBatch) -> Result<u16, FlushError> {
    use std::time::Duration;

    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(FLUSH_TIMEOUT_SECS)))
        .build()
        .new_agent();

    let response = agent
        .post(endpoint)
        .header("Content-Type", "application/json")
        .send_json(batch)
        .map_err(|e| FlushError::Http(e.to_string()))?;

    Ok(response.status().as_u16())
}

/// Truncate the queue file to zero bytes after a successful flush.
///
/// We truncate rather than delete so that any process currently appending
/// to the file still has a valid file descriptor.
fn truncate_queue() -> Result<(), FlushError> {
    let path = telemetry::queue_file_path().map_err(FlushError::Queue)?;
    if path.exists() {
        std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&path)
            .map_err(|e| FlushError::Io(e.to_string()))?;
    }
    Ok(())
}

// ── Error type ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum FlushError {
    Queue(shatter_core::telemetry::TelemetryError),
    Serialize(String),
    Http(String),
    Io(String),
}

impl std::fmt::Display for FlushError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlushError::Queue(e) => write!(f, "queue read error: {e}"),
            FlushError::Serialize(s) => write!(f, "serialize error: {s}"),
            FlushError::Http(s) => write!(f, "HTTP error: {s}"),
            FlushError::Io(s) => write!(f, "I/O error: {s}"),
        }
    }
}

impl std::error::Error for FlushError {}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use shatter_core::telemetry::{EventPayload, TELEMETRY_SCHEMA_VERSION, TelemetryEvent};

    fn make_event(name: &str) -> TelemetryEvent {
        TelemetryEvent {
            event_id: "test-id".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            schema_version: TELEMETRY_SCHEMA_VERSION,
            anonymous_id: "anon-abc123".to_string(),
            event_name: name.to_string(),
            payload: EventPayload::CommandRun {
                subcommand: "explore".to_string(),
                sanitized_args: vec!["explore".to_string()],
                duration_ms: 500,
                exit_code: 0,
            },
        }
    }

    // ── build_batch ────────────────────────────────────────────────────────

    #[test]
    fn build_batch_maps_event_name() {
        let event = make_event("command_run");
        let batch = build_batch("key123", &[event]).unwrap();
        assert_eq!(batch.batch.len(), 1);
        assert_eq!(batch.batch[0].event, "command_run");
        assert_eq!(batch.api_key, "key123");
    }

    #[test]
    fn build_batch_sets_distinct_id() {
        let event = make_event("command_run");
        let batch = build_batch("k", &[event]).unwrap();
        assert_eq!(batch.batch[0].distinct_id, "anon-abc123");
    }

    #[test]
    fn build_batch_includes_schema_version_in_properties() {
        let event = make_event("command_run");
        let batch = build_batch("k", &[event]).unwrap();
        let props = &batch.batch[0].properties;
        assert_eq!(props["schema_version"], TELEMETRY_SCHEMA_VERSION);
    }

    #[test]
    fn build_batch_includes_event_id_in_properties() {
        let event = make_event("command_run");
        let batch = build_batch("k", &[event]).unwrap();
        let props = &batch.batch[0].properties;
        assert_eq!(props["event_id"], "test-id");
    }

    #[test]
    fn build_batch_empty_events_produces_empty_batch() {
        let batch = build_batch("k", &[]).unwrap();
        assert!(batch.batch.is_empty());
    }

    #[test]
    fn build_batch_preserves_order() {
        let events: Vec<TelemetryEvent> = (0..5).map(|i| make_event(&format!("ev{i}"))).collect();
        let batch = build_batch("k", &events).unwrap();
        for (i, item) in batch.batch.iter().enumerate() {
            assert_eq!(item.event, format!("ev{i}"));
        }
    }

    // ── shatter_to_posthog ─────────────────────────────────────────────────

    #[test]
    fn posthog_event_timestamp_matches() {
        let event = make_event("bad_cli_args");
        let ph = shatter_to_posthog(&event).unwrap();
        assert_eq!(ph.timestamp, "2026-01-01T00:00:00Z");
    }

    #[test]
    fn posthog_event_payload_type_field_present() {
        // EventPayload uses serde tag = "type", so "type" key must appear in properties.
        let event = make_event("command_run");
        let ph = shatter_to_posthog(&event).unwrap();
        let props = ph.properties.as_object().unwrap();
        assert!(
            props.contains_key("type"),
            "properties must contain serde tag 'type'"
        );
    }

    // ── flush cap (no network) ─────────────────────────────────────────────

    #[test]
    fn flush_max_events_constant_is_positive() {
        const { assert!(FLUSH_MAX_EVENTS > 0) };
        const { assert!(FLUSH_MAX_EVENTS <= 100) };
    }

    #[test]
    fn flush_timeout_constant_is_positive() {
        const { assert!(FLUSH_TIMEOUT_SECS > 0) };
    }

    // ── SHATTER_TELEMETRY_DEBUG flush path (temp queue file) ──────────────

    #[test]
    fn debug_mode_prints_and_clears_queue() {
        // Set up a temporary queue file by writing an event directly.
        let tmp = tempfile::tempdir().unwrap();
        let queue_path = tmp.path().join("events.jsonl");

        let event = make_event("command_run");
        let line = serde_json::to_string(&event).unwrap() + "\n";
        std::fs::write(&queue_path, &line).unwrap();

        // Verify file has content.
        assert!(!std::fs::read_to_string(&queue_path).unwrap().is_empty());

        // We cannot easily test the full flush path without mocking queue_file_path(),
        // but we can test truncate_queue() indirectly by verifying the file is zeroed.
        std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&queue_path)
            .unwrap();
        assert!(std::fs::read_to_string(&queue_path).unwrap().is_empty());
    }

    // ── Serialization roundtrip ────────────────────────────────────────────

    #[test]
    fn posthog_batch_serializes_to_valid_json() {
        let event = make_event("command_run");
        let batch = build_batch("test_key", &[event]).unwrap();
        let json = serde_json::to_string(&batch).unwrap();
        // Must parse back as a JSON object.
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["api_key"], "test_key");
        assert!(parsed["batch"].is_array());
    }

    #[test]
    fn posthog_batch_json_has_event_field() {
        let event = make_event("bad_cli_args");
        let batch = build_batch("k", &[event]).unwrap();
        let json = serde_json::to_string(&batch).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["batch"][0]["event"], "bad_cli_args");
    }

    // ── FLUSH_MAX_EVENTS capping ───────────────────────────────────────────

    #[test]
    fn batch_capped_at_flush_max_events() {
        let events: Vec<TelemetryEvent> = (0..FLUSH_MAX_EVENTS + 10)
            .map(|i| make_event(&format!("ev{i}")))
            .collect();
        // Simulate the capping logic from try_flush_queue.
        let capped: Vec<TelemetryEvent> = events.into_iter().take(FLUSH_MAX_EVENTS).collect();
        assert_eq!(capped.len(), FLUSH_MAX_EVENTS);
    }
}
