use shatter_core::telemetry;

use crate::args::TelemetryAction;

/// Number of hex characters to display from the anonymous ID.
const ANONYMOUS_ID_DISPLAY_LEN: usize = 12;

pub(crate) fn run_telemetry(action: &TelemetryAction) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        TelemetryAction::Status => print_status(),
        TelemetryAction::Off => set_enabled(false),
        TelemetryAction::On => set_enabled(true),
        TelemetryAction::ResetId => reset_id(),
    }
}

fn print_status() -> Result<(), Box<dyn std::error::Error>> {
    let enabled = telemetry::is_enabled();
    let config_path = telemetry::config_file_path()?;
    let queue_path = telemetry::queue_file_path()?;

    let anon_id = telemetry::generate_anonymous_id()
        .map(|id| {
            let truncated: String = id.chars().take(ANONYMOUS_ID_DISPLAY_LEN).collect();
            format!("{}…", truncated)
        })
        .unwrap_or_else(|_| "<unavailable>".to_string());

    let event_count = telemetry::read_queue()
        .map(|events| events.len())
        .unwrap_or(0);

    println!("Telemetry:    {}", if enabled { "enabled" } else { "disabled" });
    println!("Config:       {}", config_path.display());
    println!("Anonymous ID: {}", anon_id);
    println!("Queue:        {} ({} events)", queue_path.display(), event_count);

    Ok(())
}

fn set_enabled(enabled: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = telemetry::read_config().unwrap_or_default();
    config.enabled = enabled;
    telemetry::write_config(&config)?;

    let state = if enabled { "enabled" } else { "disabled" };
    println!("Telemetry {}.", state);
    Ok(())
}

fn reset_id() -> Result<(), Box<dyn std::error::Error>> {
    // Delete the salt file so generate_anonymous_id() creates a fresh one on next call.
    let salt_path = telemetry::salt_file_path()?;
    if salt_path.exists() {
        std::fs::remove_file(&salt_path)?;
    }

    // Regenerate immediately so the user sees the new ID.
    let new_id = telemetry::generate_anonymous_id()?;
    let truncated: String = new_id.chars().take(ANONYMOUS_ID_DISPLAY_LEN).collect();
    println!("Anonymous ID regenerated: {}…", truncated);
    Ok(())
}
