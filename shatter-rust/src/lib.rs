pub mod adapters;
pub mod analyzer;
pub mod executor;
pub mod generators;
pub mod handler;
pub mod instrument;
pub mod protocol;
pub mod setup;
pub mod timing;
pub mod wasm_generator;

/// Process-global mutex used by tests that mutate environment variables.
///
/// `std::env::set_var` / `remove_var` affect the whole process, so tests that
/// write env vars must hold this lock for the duration of the write+read cycle
/// to avoid racing with other env-var tests running in the same test binary.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
