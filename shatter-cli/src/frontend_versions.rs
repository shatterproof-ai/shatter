//! Single source of truth for the build-time frontend version hashes.
//!
//! `build.rs` bakes a hash of each embedded frontend into the binary (str-o09e).
//! These same hashes are the self-describing versions `shatter --version` and
//! `shatter doctor` print, and they key the analysis cache so a frontend whose
//! analyze behavior changed for unchanged source invalidates stale entries
//! (str-2cihu). Every consumer reads them from here rather than re-invoking
//! `env!` inline, so there is one place the mapping lives.

use std::collections::HashMap;

use shatter_core::discovery::Language;

/// Build-time hash of the Go frontend source tree (see `build.rs`).
pub(crate) const GO_FRONTEND_SOURCE_HASH: &str = env!("GO_FRONTEND_SOURCE_HASH");
/// Build-time hash of the compiled Go frontend binary that is embedded.
pub(crate) const GO_FRONTEND_BINARY_HASH: &str = env!("GO_FRONTEND_HASH");
/// Build-time hash of the embedded TypeScript bundle pair.
pub(crate) const TS_FRONTEND_BUNDLE_HASH: &str = env!("FRONTEND_BUNDLE_HASH");
/// Path to the `shatter-go/` source tree at build time. In an installed binary
/// this directory typically does not exist, in which case staleness checks are
/// skipped.
pub(crate) const GO_FRONTEND_SOURCE_DIR: &str = env!("GO_FRONTEND_SOURCE_DIR");

/// Per-language frontend analyzer versions, folded into the analysis cache key
/// (str-2cihu). Each value is the build-time source/bundle hash of that
/// frontend; when it changes, prior cached analysis entries for that language
/// are invalidated instead of serving stale results.
///
/// Languages without an embedded source hash (currently Rust, an externally
/// installed frontend) are omitted; `batch_analyze` treats a missing entry as
/// an empty version, preserving the pre-str-2cihu keying for them.
pub(crate) fn analyzer_versions() -> HashMap<Language, String> {
    let mut versions = HashMap::new();
    versions.insert(Language::Go, GO_FRONTEND_SOURCE_HASH.to_string());
    versions.insert(Language::TypeScript, TS_FRONTEND_BUNDLE_HASH.to_string());
    versions
}
