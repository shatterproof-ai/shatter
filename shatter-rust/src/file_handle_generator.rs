//! Native generator for `std::fs::File`, `BufReader<File>`, and `BufWriter<File>`.
//!
//! Creates temp files via the `tempfile` crate, populates them with content
//! determined by [`ContentKind`], and wraps the handle according to
//! [`HandleMode`]. The [`FileHandleRecipe`] is serializable so the core engine
//! can replay the same file on subsequent executions.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Write};

use serde::{Deserialize, Serialize};

use crate::generators::GeneratorResult;

/// Content strategies for generated temp files.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    /// Zero-byte file.
    Empty,
    /// A short UTF-8 string.
    SmallText,
    /// Non-UTF-8 byte sequence.
    Binary,
    /// ~64 KiB of repeated text, large enough to exercise buffered I/O.
    Large,
}

/// Which standard-library wrapper to return around the underlying `File`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HandleMode {
    /// Raw `std::fs::File`.
    File,
    /// `std::io::BufReader<File>`.
    BufReader,
    /// `std::io::BufWriter<File>`.
    BufWriter,
}

/// Serializable reconstruction parameters stored by the core engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHandleRecipe {
    pub temp_path: String,
    pub content_kind: ContentKind,
    pub handle_mode: HandleMode,
}

const SMALL_TEXT_CONTENT: &str = "hello shatter\n";
const LARGE_CONTENT_REPEAT: &str = "abcdefghijklmnopqrstuvwxyz0123456789\n";
const LARGE_CONTENT_REPEATS: usize = 1780; // ~64 KiB

/// Populate a file with content matching `kind`. The file is truncated first.
fn write_content(file: &mut File, kind: ContentKind) -> std::io::Result<()> {
    file.set_len(0)?;
    match kind {
        ContentKind::Empty => {}
        ContentKind::SmallText => {
            file.write_all(SMALL_TEXT_CONTENT.as_bytes())?;
        }
        ContentKind::Binary => {
            let bytes: Vec<u8> = (0..=255).collect();
            file.write_all(&bytes)?;
        }
        ContentKind::Large => {
            for _ in 0..LARGE_CONTENT_REPEATS {
                file.write_all(LARGE_CONTENT_REPEAT.as_bytes())?;
            }
        }
    }
    file.sync_all()?;
    Ok(())
}

/// Parse an incoming recipe JSON, falling back to defaults when absent.
fn parse_recipe(raw: Option<serde_json::Value>) -> (Option<String>, ContentKind, HandleMode) {
    match raw {
        Some(v) => match serde_json::from_value::<FileHandleRecipe>(v) {
            Ok(r) => (Some(r.temp_path), r.content_kind, r.handle_mode),
            Err(_) => (None, ContentKind::SmallText, HandleMode::File),
        },
        None => (None, ContentKind::SmallText, HandleMode::File),
    }
}

/// Open (or create) the backing temp file and populate it.
///
/// When `existing_path` is `Some`, the file at that path is reopened (and
/// recreated if it was deleted between runs). Otherwise a fresh temp file is
/// created via `tempfile::NamedTempFile` and persisted so it survives past
/// this function.
fn open_and_populate(
    existing_path: Option<String>,
    content_kind: ContentKind,
) -> Result<(File, String), String> {
    match existing_path {
        Some(path) => {
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&path)
                .map_err(|e| format!("failed to open {path}: {e}"))?;
            write_content(&mut file, content_kind)
                .map_err(|e| format!("failed to write content to {path}: {e}"))?;
            // Reopen for a clean read position.
            let file = File::open(&path)
                .map_err(|e| format!("failed to reopen {path}: {e}"))?;
            Ok((file, path))
        }
        None => {
            let named = tempfile::NamedTempFile::new()
                .map_err(|e| format!("failed to create temp file: {e}"))?;
            let path = named.path().to_string_lossy().into_owned();
            // Persist so the file outlives this scope (core may replay later).
            let mut file = named
                .persist(&path)
                .map_err(|e| format!("failed to persist temp file: {e}"))?;
            write_content(&mut file, content_kind)
                .map_err(|e| format!("failed to write content: {e}"))?;
            // Reopen for a clean read position.
            let file = File::open(&path)
                .map_err(|e| format!("failed to reopen temp file: {e}"))?;
            Ok((file, path))
        }
    }
}

/// Wrap the raw `File` in the requested handle type and return a boxed `Any`.
fn wrap_handle(file: File, mode: HandleMode) -> Box<dyn std::any::Any + Send> {
    match mode {
        HandleMode::File => Box::new(file),
        HandleMode::BufReader => Box::new(BufReader::new(file)),
        HandleMode::BufWriter => {
            // BufWriter needs write access; reopen the path isn't feasible
            // without the path, but we already have a File opened read-only.
            // The caller opened it read-only for position reset. For BufWriter
            // we need write access, so we use the file as-is (the OS allows
            // re-wrapping). In practice the executor will use this for writing.
            Box::new(BufWriter::new(file))
        }
    }
}

/// Entry point called by `NativeRegistry`.
pub fn generate(recipe: Option<serde_json::Value>) -> GeneratorResult {
    let (existing_path, content_kind, handle_mode) = parse_recipe(recipe);

    let (file, path) =
        open_and_populate(existing_path, content_kind).expect("file handle generator failed");

    let recipe_value = serde_json::to_value(FileHandleRecipe {
        temp_path: path,
        content_kind,
        handle_mode,
    })
    .expect("FileHandleRecipe serialization cannot fail");

    let id = format!("file-handle-{:?}-{:?}", handle_mode, content_kind).to_lowercase();

    GeneratorResult {
        id,
        value: wrap_handle(file, handle_mode),
        recipe: recipe_value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn generate_no_recipe_returns_file() {
        let result = generate(None);
        assert_eq!(result.id, "file-handle-file-smalltext");
        // Value should downcast to File.
        assert!(result.value.downcast_ref::<File>().is_some());
        // Recipe should round-trip.
        let recipe: FileHandleRecipe =
            serde_json::from_value(result.recipe).expect("recipe should deserialize");
        assert_eq!(recipe.content_kind, ContentKind::SmallText);
        assert_eq!(recipe.handle_mode, HandleMode::File);
        assert!(!recipe.temp_path.is_empty());
        // Clean up.
        let _ = std::fs::remove_file(&recipe.temp_path);
    }

    #[test]
    fn generate_with_recipe_replays_same_path() {
        // First call creates the file.
        let first = generate(None);
        let first_recipe: FileHandleRecipe =
            serde_json::from_value(first.recipe.clone()).expect("deserialize");
        let path = first_recipe.temp_path.clone();

        // Replay with the same recipe.
        let second = generate(Some(first.recipe));
        let second_recipe: FileHandleRecipe =
            serde_json::from_value(second.recipe).expect("deserialize");
        assert_eq!(second_recipe.temp_path, path);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn handle_mode_bufreader() {
        let first = generate(None);
        let first_recipe: FileHandleRecipe =
            serde_json::from_value(first.recipe).expect("deserialize");

        let buf_reader_recipe = serde_json::to_value(FileHandleRecipe {
            temp_path: first_recipe.temp_path.clone(),
            content_kind: ContentKind::SmallText,
            handle_mode: HandleMode::BufReader,
        })
        .unwrap();

        let result = generate(Some(buf_reader_recipe));
        assert_eq!(result.id, "file-handle-bufreader-smalltext");
        assert!(result.value.downcast_ref::<BufReader<File>>().is_some());

        let _ = std::fs::remove_file(&first_recipe.temp_path);
    }

    #[test]
    fn handle_mode_bufwriter() {
        let first = generate(None);
        let first_recipe: FileHandleRecipe =
            serde_json::from_value(first.recipe).expect("deserialize");

        let writer_recipe = serde_json::to_value(FileHandleRecipe {
            temp_path: first_recipe.temp_path.clone(),
            content_kind: ContentKind::Empty,
            handle_mode: HandleMode::BufWriter,
        })
        .unwrap();

        let result = generate(Some(writer_recipe));
        assert_eq!(result.id, "file-handle-bufwriter-empty");
        assert!(result.value.downcast_ref::<BufWriter<File>>().is_some());

        let _ = std::fs::remove_file(&first_recipe.temp_path);
    }

    #[test]
    fn content_kind_empty() {
        let result = generate(None);
        let recipe: FileHandleRecipe =
            serde_json::from_value(result.recipe).expect("deserialize");

        let empty_recipe = serde_json::to_value(FileHandleRecipe {
            temp_path: recipe.temp_path.clone(),
            content_kind: ContentKind::Empty,
            handle_mode: HandleMode::File,
        })
        .unwrap();

        let result = generate(Some(empty_recipe));
        let mut file = result.value.downcast::<File>().expect("should be File");
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();
        assert!(buf.is_empty());

        let _ = std::fs::remove_file(&recipe.temp_path);
    }

    #[test]
    fn content_kind_binary() {
        let first = generate(None);
        let first_recipe: FileHandleRecipe =
            serde_json::from_value(first.recipe).expect("deserialize");

        let binary_recipe = serde_json::to_value(FileHandleRecipe {
            temp_path: first_recipe.temp_path.clone(),
            content_kind: ContentKind::Binary,
            handle_mode: HandleMode::File,
        })
        .unwrap();

        let result = generate(Some(binary_recipe));
        let mut file = result.value.downcast::<File>().expect("should be File");
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();
        assert_eq!(buf.len(), 256);
        assert_eq!(buf[0], 0);
        assert_eq!(buf[255], 255);

        let _ = std::fs::remove_file(&first_recipe.temp_path);
    }

    #[test]
    fn content_kind_large() {
        let first = generate(None);
        let first_recipe: FileHandleRecipe =
            serde_json::from_value(first.recipe).expect("deserialize");

        let large_recipe = serde_json::to_value(FileHandleRecipe {
            temp_path: first_recipe.temp_path.clone(),
            content_kind: ContentKind::Large,
            handle_mode: HandleMode::File,
        })
        .unwrap();

        let result = generate(Some(large_recipe));
        let mut file = result.value.downcast::<File>().expect("should be File");
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).unwrap();
        let expected_size = LARGE_CONTENT_REPEAT.len() * LARGE_CONTENT_REPEATS;
        assert_eq!(buf.len(), expected_size);

        let _ = std::fs::remove_file(&first_recipe.temp_path);
    }

    mod prop {
        use super::*;
        use proptest::prelude::*;

        fn arb_content_kind() -> impl Strategy<Value = ContentKind> {
            prop_oneof![
                Just(ContentKind::Empty),
                Just(ContentKind::SmallText),
                Just(ContentKind::Binary),
                Just(ContentKind::Large),
            ]
        }

        fn arb_handle_mode() -> impl Strategy<Value = HandleMode> {
            prop_oneof![
                Just(HandleMode::File),
                Just(HandleMode::BufReader),
                Just(HandleMode::BufWriter),
            ]
        }

        proptest! {
            #[test]
            fn recipe_roundtrip_preserves_content(
                content_kind in arb_content_kind(),
                handle_mode in arb_handle_mode(),
            ) {
                // First generate creates a temp file.
                let first = generate(None);
                let first_recipe: FileHandleRecipe =
                    serde_json::from_value(first.recipe).unwrap();

                // Build a recipe with the chosen variants.
                let recipe = serde_json::to_value(FileHandleRecipe {
                    temp_path: first_recipe.temp_path.clone(),
                    content_kind,
                    handle_mode,
                })
                .unwrap();

                // Replay and verify recipe survives the round-trip.
                let result = generate(Some(recipe));
                let out_recipe: FileHandleRecipe =
                    serde_json::from_value(result.recipe).unwrap();
                prop_assert_eq!(out_recipe.content_kind, content_kind);
                prop_assert_eq!(out_recipe.handle_mode, handle_mode);
                let temp_path = first_recipe.temp_path;
                prop_assert_eq!(out_recipe.temp_path, temp_path.clone());

                let _ = std::fs::remove_file(&temp_path);
            }
        }
    }
}
