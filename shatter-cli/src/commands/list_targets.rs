//! Handler for `shatter list-targets`.

use std::io::Write;
use std::path::{Path, PathBuf};

use shatter_core::scope::{find_scope_config, ScopeConfig, ScopeMatcher};
use shatter_core::target_manifest::{
    ExcludedFileEntry, ExclusionReason, TargetManifest, TargetManifestConfig,
};

use crate::args::{ListTargetsArgs, ListTargetsFormat};

pub(crate) fn run(args: &ListTargetsArgs) -> Result<(), String> {
    let root = args.directory.canonicalize().map_err(|e| {
        format!(
            "cannot resolve directory '{}': {e}",
            args.directory.display()
        )
    })?;

    let scope = load_scope(args.scope.as_deref(), &root)?;
    let config = TargetManifestConfig {
        include: args.include.clone(),
        exclude: args.exclude.clone(),
        language: args.language.clone(),
        max_depth: None,
    };

    let mut manifest = TargetManifest::build(&root, &config)
        .map_err(|e| format!("failed to build target manifest: {e}"))?;
    if let Some(scope) = scope.as_ref() {
        apply_scope(&mut manifest, scope, &root)?;
    }

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

struct Scope {
    matcher: ScopeMatcher,
    root: PathBuf,
}

fn load_scope(scope_path: Option<&Path>, scan_root: &Path) -> Result<Option<Scope>, String> {
    let (config, root) = match scope_path {
        Some(path) => {
            let path = path
                .canonicalize()
                .map_err(|e| format!("cannot resolve scope config '{}': {e}", path.display()))?;
            let root = path
                .parent()
                .ok_or_else(|| {
                    format!("scope config '{}' has no parent directory", path.display())
                })?
                .to_path_buf();
            let config = ScopeConfig::from_file(&path)
                .map_err(|e| format!("failed to load scope config: {e}"))?;
            (config, root)
        }
        None => match find_scope_config(scan_root)
            .map_err(|e| format!("failed to load scope config: {e}"))?
        {
            Some((config, root)) => (config, root),
            None => return Ok(None),
        },
    };
    let matcher = ScopeMatcher::new(&config).map_err(|e| format!("invalid scope config: {e}"))?;
    Ok(Some(Scope { matcher, root }))
}

fn apply_scope(
    manifest: &mut TargetManifest,
    scope: &Scope,
    scan_root: &Path,
) -> Result<(), String> {
    let scan_prefix = scan_root.strip_prefix(&scope.root).map_err(|_| {
        format!(
            "scope config root '{}' is not an ancestor of scan directory '{}'",
            scope.root.display(),
            scan_root.display()
        )
    })?;
    let mut selected = Vec::new();
    let mut excluded = Vec::new();
    for entry in std::mem::take(&mut manifest.selected) {
        let scope_path = scan_prefix.join(&entry.path);
        let scope_path = scope_path
            .to_str()
            .ok_or_else(|| format!("scope path '{}' is not valid UTF-8", scope_path.display()))?;
        if scope.matcher.is_included(scope_path) {
            selected.push(entry);
        } else {
            excluded.push(ExcludedFileEntry {
                path: entry.path,
                reason: ExclusionReason::ExcludePattern,
                matched_pattern: None,
            });
        }
    }
    manifest.selected = selected;
    manifest.excluded.extend(excluded);
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::ListTargetsFormat;
    use std::path::PathBuf;

    #[test]
    fn scope_excludes_apply_when_listing_a_subdirectory_with_cli_includes() {
        let temp = tempfile::tempdir().expect("create temporary project");
        let web = temp.path().join("web");
        std::fs::create_dir(&web).expect("create web directory");
        std::fs::write(web.join("kept.ts"), "export const kept = 1;")
            .expect("write included source");
        std::fs::write(web.join("ignored.ts"), "export const ignored = 1;")
            .expect("write excluded source");
        let scope = temp.path().join("shatter.scope.yaml");
        std::fs::write(&scope, "scope:\n  exclude:\n    - web/ignored.ts\n")
            .expect("write scope config");
        let output = temp.path().join("targets.json");

        run(&ListTargetsArgs {
            directory: web,
            scope: Some(scope),
            include: vec!["**/*.ts".to_string()],
            exclude: vec![],
            language: None,
            format: ListTargetsFormat::Json,
            output: Some(output.clone()),
        })
        .expect("list targets");

        let manifest: shatter_core::target_manifest::TargetManifest =
            serde_json::from_str(&std::fs::read_to_string(output).expect("read manifest"))
                .expect("parse manifest");
        let selected: Vec<_> = manifest.selected.iter().map(|entry| &entry.path).collect();
        assert_eq!(selected, vec![&PathBuf::from("kept.ts")]);
    }
}
