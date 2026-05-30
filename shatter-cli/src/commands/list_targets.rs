//! Handler for `shatter list-targets`.

use std::io::Write;

use shatter_core::target_manifest::{TargetManifest, TargetManifestConfig};

use crate::args::{ListTargetsArgs, ListTargetsFormat};

pub(crate) fn run(args: &ListTargetsArgs) -> Result<(), String> {
    let root = args
        .directory
        .canonicalize()
        .map_err(|e| format!("cannot resolve directory '{}': {e}", args.directory.display()))?;

    let config = TargetManifestConfig {
        include: args.include.clone(),
        exclude: args.exclude.clone(),
        language: args.language.clone(),
        max_depth: None,
    };

    let manifest = TargetManifest::build(&root, &config)
        .map_err(|e| format!("failed to build target manifest: {e}"))?;

    let output: Box<dyn Write> = if let Some(ref path) = args.output {
        match std::fs::File::create(path) {
            Ok(f) => Box::new(f),
            Err(e) => return Err(format!("cannot open output file '{}': {e}", path.display())),
        }
    } else {
        Box::new(std::io::stdout())
    };

    write_manifest(output, &manifest, args.format, args.output.as_deref())
        .map_err(|e| format!("failed to write output: {e}"))
}

fn write_manifest(
    mut w: impl Write,
    manifest: &TargetManifest,
    format: ListTargetsFormat,
    output_path: Option<&std::path::Path>,
) -> std::io::Result<()> {
    match format {
        ListTargetsFormat::Json => {
            // When writing to a file, also use atomic write.
            if let Some(path) = output_path {
                manifest
                    .write_json(path)
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
            } else {
                let json = serde_json::to_string_pretty(manifest)
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                writeln!(w, "{json}")?;
            }
        }
        ListTargetsFormat::Markdown => {
            write!(w, "{}", manifest.render_markdown())?;
        }
        ListTargetsFormat::Text => {
            write!(w, "{}", manifest.render_text())?;
        }
    }
    Ok(())
}
