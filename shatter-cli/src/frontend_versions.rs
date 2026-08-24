//! Single source of truth for the build-time frontend version hashes.
//!
//! `build.rs` bakes a hash of each embedded frontend into the binary (str-o09e).
//! These same hashes are the self-describing versions `shatter --version` and
//! `shatter doctor` print, and they key the analysis cache so a frontend whose
//! analyze behavior changed for unchanged source invalidates stale entries
//! (str-2cihu). Every consumer reads them from here rather than re-invoking
//! `env!` inline, so there is one place the mapping lives.

use std::fs::File;
use std::io::{BufReader, Read};

use shatter_core::discovery::Language;
use shatter_core::frontend::FrontendConfig;
use sha2::{Digest, Sha256};

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

/// Identity of the analyzer selected by a resolved frontend configuration.
///
/// TypeScript always executes the embedded bundle, whose build-time hash is
/// already available. Go and Rust may execute project-local or externally
/// installed binaries, so their actual executable bytes define the cache
/// identity. This prevents a replaced custom frontend from reusing analysis
/// produced by its predecessor.
pub(crate) fn analyzer_version(
    language: Language,
    config: &FrontendConfig,
) -> Result<String, String> {
    match language {
        Language::TypeScript => Ok(TS_FRONTEND_BUNDLE_HASH.to_string()),
        Language::Go | Language::Rust => hash_file(&config.command),
    }
}

fn hash_file(path: &std::path::Path) -> Result<String, String> {
    let file = File::open(path)
        .map_err(|error| format!("failed to open frontend {}: {error}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("failed to read frontend {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use shatter_core::frontend::FrontendConfig;

    use super::*;

    #[test]
    fn external_analyzer_version_tracks_executable_contents() {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("shatter-rust");
        fs::write(&executable, b"first analyzer").unwrap();
        let config = FrontendConfig::new(executable.clone());

        let first = analyzer_version(Language::Rust, &config).unwrap();
        let repeated = analyzer_version(Language::Rust, &config).unwrap();
        fs::write(executable, b"second analyzer").unwrap();
        let changed = analyzer_version(Language::Rust, &config).unwrap();

        assert_eq!(first, repeated);
        assert_ne!(first, changed);
    }

    #[test]
    fn typescript_analyzer_version_uses_embedded_bundle_hash() {
        let config = FrontendConfig::new("node".into());

        assert_eq!(
            analyzer_version(Language::TypeScript, &config).unwrap(),
            TS_FRONTEND_BUNDLE_HASH
        );
    }
}
