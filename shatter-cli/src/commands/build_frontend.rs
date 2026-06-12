use std::path::{Path, PathBuf};

use shatter_core::config::{self as shatter_config, ShatterConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeGeneratorConfig {
    name: String,
    path: PathBuf,
}

/// Build a custom frontend binary with user-provided native generators.
pub(crate) fn run_build_frontend(
    language: &str,
    config_dir: Option<&Path>,
    output_dir: Option<&Path>,
) -> Result<(), String> {
    let shatter_dir = config_dir
        .map(PathBuf::from)
        .or_else(|| {
            let candidate = PathBuf::from(".shatter");
            candidate.is_dir().then_some(candidate)
        })
        .ok_or_else(|| {
            "no .shatter/ directory found; pass --config or run from project root".to_string()
        })?;
    let shatter_dir = std::fs::canonicalize(&shatter_dir).unwrap_or(shatter_dir);

    let config_path = shatter_dir.join("config.yaml");
    if !config_path.exists() {
        return Err(format!("config not found: {}", config_path.display()));
    }

    let config = shatter_config::parse_config(&config_path)
        .map_err(|e| format!("failed to load config: {e}"))?;

    let out_dir = output_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".shatter-cache").join("bin"));

    std::fs::create_dir_all(&out_dir).map_err(|e| format!("failed to create output dir: {e}"))?;

    match language {
        "go" => build_go_frontend(&shatter_dir, &config, &out_dir),
        "rust" => build_rust_frontend(&shatter_dir, &config, &out_dir),
        other => Err(format!(
            "unsupported language '{other}'; supported: go, rust"
        )),
    }
}

/// Collect native generator file paths from config for a given language extension.
fn collect_native_generators(
    shatter_dir: &Path,
    config: &ShatterConfig,
    extension: &str,
    include_function_level: bool,
) -> Vec<NativeGeneratorConfig> {
    let mut generators = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let check = |name: &str, path_str: &str| {
        let path = Path::new(path_str);
        if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            let resolved = if path.is_absolute() {
                path.to_path_buf()
            } else {
                shatter_dir.join(path)
            };
            Some(NativeGeneratorConfig {
                name: name.to_string(),
                path: resolved,
            })
        } else {
            None
        }
    };

    let mut push = |entry: NativeGeneratorConfig| {
        if seen.insert((entry.name.clone(), entry.path.clone())) {
            generators.push(entry);
        }
    };

    if let Some(ref type_gens) = config.defaults.generators {
        for (name, path_str) in type_gens {
            if let Some(entry) = check(name, path_str) {
                push(entry);
            }
        }
    }
    if let Some(ref param_gens) = config.defaults.param_generators {
        for (name, path_str) in param_gens {
            if let Some(entry) = check(name, path_str) {
                push(entry);
            }
        }
    }
    if include_function_level {
        for function_config in config.functions.values() {
            if let Some(ref type_gens) = function_config.generators {
                for (name, path_str) in type_gens {
                    if let Some(entry) = check(name, path_str) {
                        push(entry);
                    }
                }
            }
            if let Some(ref param_gens) = function_config.param_generators {
                for (name, path_str) in param_gens {
                    if let Some(entry) = check(name, path_str) {
                        push(entry);
                    }
                }
            }
        }
    }
    generators
}

fn extract_package_name(cargo_toml: &str) -> Option<String> {
    let mut in_package = false;
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_package = false;
        }
        if !in_package || !trimmed.starts_with("name") {
            continue;
        }
        let (_, value) = trimmed.split_once('=')?;
        return Some(value.trim().trim_matches('"').to_string());
    }
    None
}

fn rust_project_dependency(project_root: &Path) -> Option<String> {
    let manifest = project_root.join("Cargo.toml");
    let cargo_toml = std::fs::read_to_string(&manifest).ok()?;
    let package_name = extract_package_name(&cargo_toml)?;
    let project_path = std::fs::canonicalize(project_root)
        .unwrap_or_else(|_| project_root.to_path_buf())
        .display()
        .to_string()
        .replace('\\', "/");
    Some(format!(
        "{package_name} = {{ path = \"{project_path}\" }}\n"
    ))
}

fn rust_project_dependencies(project_root: &Path) -> String {
    let manifest = project_root.join("Cargo.toml");
    let Ok(cargo_toml) = std::fs::read_to_string(&manifest) else {
        return String::new();
    };
    let mut in_dependencies = false;
    let mut deps = String::new();
    for line in cargo_toml.lines() {
        let trimmed = line.trim();
        if trimmed == "[dependencies]" {
            in_dependencies = true;
            continue;
        }
        if trimmed.starts_with('[') {
            in_dependencies = false;
        }
        if !in_dependencies || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let key = trimmed.split(['=', ' ', '.']).next().unwrap_or("").trim();
        if matches!(key, "serde_json" | "shatter-rust") {
            continue;
        }
        deps.push_str(line);
        deps.push('\n');
    }
    deps
}

fn rust_frontend_project_deps(project_root: &Path) -> String {
    let mut deps = rust_project_dependency(project_root).unwrap_or_default();
    deps.push_str(&rust_project_dependencies(project_root));
    deps
}

fn project_cargo_lock_source(project_root: &Path) -> Option<PathBuf> {
    let project_lock = project_root.join("Cargo.lock");
    if project_lock.exists() {
        return Some(project_lock);
    }

    let mut dir = project_root.parent()?;
    loop {
        let manifest = dir.join("Cargo.toml");
        if manifest.exists()
            && std::fs::read_to_string(&manifest)
                .ok()
                .is_some_and(|content| {
                    content.lines().any(|line| {
                        line.trim() == "[workspace]" || line.trim().starts_with("[workspace]")
                    })
                })
        {
            let workspace_lock = dir.join("Cargo.lock");
            return workspace_lock.exists().then_some(workspace_lock);
        }
        if dir.parent().is_none_or(|parent| parent == dir) {
            return None;
        }
        dir = dir.parent().unwrap();
    }
}

fn copy_project_cargo_lock(project_root: &Path, build_dir: &Path) -> Result<(), String> {
    if let Some(lock_source) = project_cargo_lock_source(project_root) {
        std::fs::copy(&lock_source, build_dir.join("Cargo.lock")).map_err(|e| {
            format!(
                "failed to copy Cargo.lock from {}: {e}",
                lock_source.display()
            )
        })?;
    }
    Ok(())
}

fn rust_string_literal(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

/// Build a custom Go frontend binary with native generators compiled in.
///
/// Creates a temporary Go module that imports shatter-go's protocol handler and
/// the user's generator files, wires them into the registry, then builds a single
/// binary. Inspired by the xcaddy pattern: replace directives point to local source.
fn build_go_frontend(
    shatter_dir: &Path,
    config: &ShatterConfig,
    out_dir: &Path,
) -> Result<(), String> {
    let native_gens = collect_native_generators(shatter_dir, config, "go", false);
    if native_gens.is_empty() {
        return Err("no .go generators found in config.yaml; nothing to build".to_string());
    }

    let output_binary = out_dir.join("shatter-go-custom");
    log::info!(
        "building custom Go frontend with {} native generator(s)...",
        native_gens.len()
    );

    // Locate the shatter-go source directory. In development it's a sibling of
    // shatter-cli; for installed binaries it would be embedded (future work).
    let shatter_go_dir = locate_shatter_go_source().ok_or_else(|| {
        "cannot find shatter-go source directory; \
            ensure you are running from the shatter repo or set SHATTER_GO_SRC"
            .to_string()
    })?;

    // Create temp build directory
    let temp_dir = tempfile::tempdir().map_err(|e| format!("failed to create temp dir: {e}"))?;
    let build_dir = temp_dir.path();

    // Initialize Go module
    run_go_cmd(build_dir, &["mod", "init", "shatter-custom-frontend"])?;

    // Copy user generator files into a usergens/ package in the build dir.
    let usergens_dir = build_dir.join("usergens");
    std::fs::create_dir_all(&usergens_dir)
        .map_err(|e| format!("failed to create usergens dir: {e}"))?;

    // Resolve generator file paths and copy them
    let project_root = shatter_dir.parent().unwrap_or(Path::new("."));
    for generator in &native_gens {
        let src = if generator.path.is_absolute() {
            generator.path.clone()
        } else {
            project_root.join(&generator.path)
        };
        let filename = src
            .file_name()
            .ok_or_else(|| format!("generator path has no filename: {}", src.display()))?;
        std::fs::copy(&src, usergens_dir.join(filename))
            .map_err(|e| format!("failed to copy {}: {e}", src.display()))?;
    }

    // Ensure the usergens directory has a valid package declaration.
    // Write a package file that re-exports the generator functions.
    let usergens_pkg = "package usergens\n";
    // Check if any copied file already declares `package usergens`; if not,
    // they might declare a different package. We'll write a minimal file.
    let has_package_decl = native_gens.iter().any(|generator| {
        let src = if generator.path.is_absolute() {
            generator.path.clone()
        } else {
            project_root.join(&generator.path)
        };
        std::fs::read_to_string(&src)
            .map(|s| s.contains("package usergens"))
            .unwrap_or(false)
    });
    if !has_package_decl {
        std::fs::write(usergens_dir.join("doc.go"), usergens_pkg)
            .map_err(|e| format!("failed to write doc.go: {e}"))?;
    }

    // Generate main.go with generator registrations.
    let mut registrations = String::new();
    for generator in &native_gens {
        let name = &generator.name;
        registrations.push_str(&format!(
            "\thandler.Registry().RegisterNative(\"{name}\", usergens.{name})\n"
        ));
    }

    let main_go = format!(
        r#"package main

import (
	"fmt"
	"os"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"shatter-custom-frontend/usergens"
)

func main() {{
	handler := protocol.NewHandler(os.Stdin, os.Stdout, os.Stderr)
{registrations}
	if err := handler.Run(); err != nil {{
		fmt.Fprintf(os.Stderr, "[shatter-go-custom] Fatal: %v\n", err)
		os.Exit(1)
	}}
}}
"#
    );

    std::fs::write(build_dir.join("main.go"), &main_go)
        .map_err(|e| format!("failed to write main.go: {e}"))?;

    // Wire the local shatter-go source via replace directive.
    let shatter_go_abs = std::fs::canonicalize(&shatter_go_dir)
        .map_err(|e| format!("failed to canonicalize shatter-go path: {e}"))?;
    run_go_cmd(
        build_dir,
        &[
            "mod",
            "edit",
            "--require",
            "github.com/shatter-dev/shatter/shatter-go@v0.0.0",
            "--replace",
            &format!(
                "github.com/shatter-dev/shatter/shatter-go={}",
                shatter_go_abs.display()
            ),
        ],
    )?;

    // Resolve dependencies
    run_go_cmd(build_dir, &["mod", "tidy"])?;

    // Build the binary
    let release = std::env::var("SHATTER_HARNESS_RELEASE")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let output_str = output_binary.display().to_string();
    let mut go_args = vec!["build", "-buildvcs=false", "-o", &output_str];
    if release {
        go_args.extend(["-trimpath", "-ldflags", "-w -s"]);
    }
    go_args.push(".");
    let output = std::process::Command::new("go")
        .args(&go_args)
        .current_dir(build_dir)
        .output()
        .map_err(|e| format!("failed to run `go build`: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("go build failed:\n{stderr}"));
    }

    log::info!("custom Go frontend built: {}", output_binary.display());
    Ok(())
}

/// Locate the shatter-go source directory for custom builds.
/// Checks: $SHATTER_GO_SRC env var, then sibling directory relative to this binary.
fn locate_shatter_go_source() -> Option<PathBuf> {
    // Check env var first
    if let Ok(src) = std::env::var("SHATTER_GO_SRC") {
        let p = PathBuf::from(src);
        if p.join("go.mod").exists() {
            return Some(p);
        }
    }

    // Check relative to cwd (development layout)
    let candidate = PathBuf::from("shatter-go");
    if candidate.join("go.mod").exists() {
        return Some(candidate);
    }

    None
}

/// Run a Go command in the given directory, returning an error on failure.
fn run_go_cmd(dir: &Path, args: &[&str]) -> Result<(), String> {
    let output = std::process::Command::new("go")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("failed to run `go {}`: {e}", args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("go {} failed:\n{stderr}", args.join(" ")));
    }
    Ok(())
}

/// Build a custom Rust frontend binary with native generators compiled in.
///
/// Creates a temporary Cargo project that depends on shatter-rust and the user's
/// generator source files, generates registration code, and builds a single binary.
fn build_rust_frontend(
    shatter_dir: &Path,
    config: &ShatterConfig,
    out_dir: &Path,
) -> Result<(), String> {
    let native_gens = collect_native_generators(shatter_dir, config, "rs", true);
    if native_gens.is_empty() {
        return Err("no .rs generators found in config.yaml; nothing to build".to_string());
    }

    let output_binary = out_dir.join("shatter-rust-custom");
    log::info!(
        "building custom Rust frontend with {} native generator(s)...",
        native_gens.len()
    );

    // Locate shatter-rust source
    let shatter_rust_dir = locate_shatter_rust_source().ok_or_else(|| {
        "cannot find shatter-rust source directory; \
            ensure you are running from the shatter repo or set SHATTER_RUST_SRC"
            .to_string()
    })?;
    let shatter_rust_abs = std::fs::canonicalize(&shatter_rust_dir)
        .map_err(|e| format!("failed to canonicalize shatter-rust path: {e}"))?;

    // Create temp build directory
    let temp_dir = tempfile::tempdir().map_err(|e| format!("failed to create temp dir: {e}"))?;
    let build_dir = temp_dir.path();

    // Create Cargo project structure
    let src_dir = build_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| format!("failed to create src dir: {e}"))?;

    // Copy user generator files into src/
    let project_root = shatter_dir.parent().unwrap_or(Path::new("."));
    copy_project_cargo_lock(project_root, build_dir)?;
    let mut mod_declarations = String::new();
    let mut registrations = String::new();
    for (idx, generator) in native_gens.iter().enumerate() {
        let src = if generator.path.is_absolute() {
            generator.path.clone()
        } else {
            project_root.join(&generator.path)
        };
        let mod_name = format!("generator_{idx}");
        let filename = format!("{mod_name}.rs");
        std::fs::copy(&src, src_dir.join(&filename))
            .map_err(|e| format!("failed to copy {}: {e}", src.display()))?;
        mod_declarations.push_str(&format!("mod {mod_name};\n"));
        let registration_file = rust_string_literal(&src.display().to_string());
        let name = &generator.name;
        registrations.push_str(&format!(
            "    registry.register_for_file({registration_file}, \"{name}\", Box::new({mod_name}::{name}));\n"
        ));
    }

    let project_dependencies = rust_frontend_project_deps(project_root);

    // Write Cargo.toml
    let cargo_toml = format!(
        r#"[package]
name = "shatter-rust-custom"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "shatter-rust-custom"
path = "src/main.rs"

[dependencies]
shatter-rust = {{ path = "{}" }}
serde_json = "1"
{project_dependencies}
"#,
        shatter_rust_abs.display()
    );
    std::fs::write(build_dir.join("Cargo.toml"), &cargo_toml)
        .map_err(|e| format!("failed to write Cargo.toml: {e}"))?;

    // Generate main.rs
    let main_rs = format!(
        r#"use shatter_rust::generators::{{NativeRegistry, GeneratorResult}};
use shatter_rust::handler::Handler;
use std::io;

{mod_declarations}
fn main() {{
    let mut registry = NativeRegistry::new();
{registrations}
    let handler = Handler::new_with_native_registry(
        io::stdin().lock(),
        io::stdout().lock(),
        io::stderr(),
        registry,
    );
    if let Err(e) = handler.run() {{
        eprintln!("[shatter-rust-custom] Fatal: {{e}}");
        std::process::exit(1);
    }}
}}
"#
    );
    std::fs::write(src_dir.join("main.rs"), &main_rs)
        .map_err(|e| format!("failed to write main.rs: {e}"))?;

    // Build: run cargo check first for fast validation, then full build
    let release = std::env::var("SHATTER_HARNESS_RELEASE")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let skip_check = std::env::var("SHATTER_SKIP_CHECK")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if !skip_check {
        let mut check_args = vec!["check"];
        if release {
            check_args.push("--release");
        }
        let check_output = std::process::Command::new("cargo")
            .args(&check_args)
            .current_dir(build_dir)
            .output()
            .map_err(|e| format!("failed to run `cargo check`: {e}"))?;

        if !check_output.status.success() {
            let stderr = String::from_utf8_lossy(&check_output.stderr);
            return Err(format!("cargo check failed:\n{stderr}"));
        }
    }

    let mut cargo_args = vec!["build"];
    if release {
        cargo_args.push("--release");
    }
    let output = std::process::Command::new("cargo")
        .args(&cargo_args)
        .current_dir(build_dir)
        .output()
        .map_err(|e| format!("failed to run `cargo build`: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cargo build failed:\n{stderr}"));
    }

    // Copy the built binary to the output directory
    let profile_dir = if release { "release" } else { "debug" };
    let built = build_dir
        .join("target")
        .join(profile_dir)
        .join("shatter-rust-custom");
    std::fs::copy(&built, &output_binary).map_err(|e| format!("failed to copy binary: {e}"))?;

    log::info!("custom Rust frontend built: {}", output_binary.display());
    Ok(())
}

/// Locate the shatter-rust source directory for custom builds.
fn locate_shatter_rust_source() -> Option<PathBuf> {
    if let Ok(src) = std::env::var("SHATTER_RUST_SRC") {
        let p = PathBuf::from(src);
        if p.join("Cargo.toml").exists() {
            return Some(p);
        }
    }

    let candidate = PathBuf::from("shatter-rust");
    if candidate.join("Cargo.toml").exists() {
        return Some(candidate);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use shatter_core::config::{DefaultsConfig, FunctionConfig};
    use std::collections::HashMap;

    #[test]
    fn collect_native_generators_includes_function_level_entries() {
        let shatter_dir = Path::new(".shatter");
        let defaults = DefaultsConfig {
            generators: Some(HashMap::from([(
                "State".to_string(),
                "generators/default.rs".to_string(),
            )])),
            param_generators: Some(HashMap::from([(
                "current".to_string(),
                "generators/current.rs".to_string(),
            )])),
            ..DefaultsConfig::default()
        };
        let function_config = FunctionConfig {
            param_generators: Some(HashMap::from([(
                "current".to_string(),
                "generators/bundle.rs".to_string(),
            )])),
            ..FunctionConfig::default()
        };
        let config = ShatterConfig {
            defaults,
            functions: HashMap::from([("**/bundles.rs::get_bundle".to_string(), function_config)]),
            ..ShatterConfig::default()
        };

        let generators = collect_native_generators(shatter_dir, &config, "rs", true);

        assert!(generators.iter().any(|generator| {
            generator.name == "State"
                && generator.path == Path::new(".shatter/generators/default.rs")
        }));
        assert!(generators.iter().any(|generator| {
            generator.name == "current"
                && generator.path == Path::new(".shatter/generators/current.rs")
        }));
        assert!(generators.iter().any(|generator| {
            generator.name == "current"
                && generator.path == Path::new(".shatter/generators/bundle.rs")
        }));
    }

    #[test]
    fn rust_custom_frontend_copies_workspace_lockfile_to_build_dir() {
        let workspace_root = tempfile::tempdir().unwrap();
        let project_root = workspace_root.path().join("api");
        let build_dir = workspace_root.path().join("custom-build");
        std::fs::create_dir_all(project_root.join(".shatter")).unwrap();
        std::fs::create_dir_all(&build_dir).unwrap();

        std::fs::write(
            workspace_root.path().join("Cargo.toml"),
            r#"[workspace]
members = ["api"]
"#,
        )
        .unwrap();
        let lockfile = "# lock carried from project workspace\nversion = 4\n";
        std::fs::write(workspace_root.path().join("Cargo.lock"), lockfile).unwrap();

        copy_project_cargo_lock(&project_root, &build_dir).unwrap();

        assert_eq!(
            std::fs::read_to_string(build_dir.join("Cargo.lock")).unwrap(),
            lockfile,
            "custom frontend build dir must inherit the project lockfile",
        );
        assert!(
            !project_root.join("Cargo.lock").exists(),
            "workspace member root must not receive a generated lockfile",
        );
    }
}
