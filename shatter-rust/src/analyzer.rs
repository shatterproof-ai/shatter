//! Static analyzer for Rust source files using the `syn` crate.
//!
//! Parses Rust source code and extracts function signatures, branch locations,
//! symbolic conditions, and external dependencies for the Shatter protocol.
//!
//! ## Limitations
//!
//! - **Same-crate cross-file type resolution only** (str-do53): when analyzing
//!   a file inside a crate, struct/enum definitions from other files of the
//!   *same crate* are resolved via a crate-wide registry (see
//!   `build_crate_type_registry`), so `use crate::module::Type` synthesizes
//!   instead of becoming `Opaque`. Types from *other crates* (dependencies) are
//!   still `Opaque`, as are bare names defined ambiguously in multiple files.
//!   Pure string-source analysis (no file path) remains single-file.
//! - **No trait resolution**: Cannot determine which trait impl a method call resolves to.
//! - **No macro expansion**: Macros are not expanded; generated code is invisible.
//! - **No const evaluation**: Constant references in conditions appear as `Unknown`.
//! - **Limited generics**: Generic type parameters `T` are reported as `Unknown`.
//!   Trait bounds are noted but don't refine the type.
//! - **Pattern matching**: Literal, range, or-pattern, and variant patterns produce
//!   symbolic conditions. Deeply nested or guarded patterns may still produce `None`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use quote::ToTokens;
use syn::spanned::Spanned;

use crate::adapters::FileContext;
use crate::protocol::{
    BinOpKind, BranchInfo, BranchType, ComplexKind, ConstValue, DependencyKind, ExternalDependency,
    FunctionAnalysis, InvocationModel, LiteralValue, ParamInfo, SymExpr, TypeInfo, UnOpKind,
};
use crate::timing::TimingCollector;

/// Error type for analysis failures.
#[derive(Debug)]
pub enum AnalyzeError {
    FileNotFound(String),
    ReadError(String),
    ParseError(String),
    FunctionNotFound(String),
}

impl std::fmt::Display for AnalyzeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileNotFound(p) => write!(f, "file not found: {p}"),
            Self::ReadError(e) => write!(f, "failed to read file: {e}"),
            Self::ParseError(e) => write!(f, "failed to parse file: {e}"),
            Self::FunctionNotFound(n) => write!(f, "function not found: {n}"),
        }
    }
}

/// Analyze a Rust source file. If `function_name` is provided, return only that function.
pub fn analyze_file(
    file_path: &Path,
    function_name: Option<&str>,
) -> Result<Vec<FunctionAnalysis>, AnalyzeError> {
    analyze_file_with_timing(file_path, function_name, None)
}

pub fn analyze_file_with_timing(
    file_path: &Path,
    function_name: Option<&str>,
    mut timing: Option<&mut TimingCollector>,
) -> Result<Vec<FunctionAnalysis>, AnalyzeError> {
    if !file_path.exists() {
        return Err(AnalyzeError::FileNotFound(file_path.display().to_string()));
    }

    let source = if let Some(timing) = timing.as_deref_mut() {
        timing.record("analyze.read", |_| {
            std::fs::read_to_string(file_path).map_err(|e| AnalyzeError::ReadError(e.to_string()))
        })?
    } else {
        std::fs::read_to_string(file_path).map_err(|e| AnalyzeError::ReadError(e.to_string()))?
    };

    let registry = build_crate_type_registry(file_path);
    analyze_source_with_timing_defs(&source, function_name, timing, Some(&registry))
}

/// Analyze Rust source code from a string.
pub fn analyze_source(
    source: &str,
    function_name: Option<&str>,
) -> Result<Vec<FunctionAnalysis>, AnalyzeError> {
    analyze_source_with_timing(source, function_name, None)
}

pub fn analyze_source_with_timing(
    source: &str,
    function_name: Option<&str>,
    timing: Option<&mut TimingCollector>,
) -> Result<Vec<FunctionAnalysis>, AnalyzeError> {
    analyze_source_with_timing_defs(source, function_name, timing, None)
}

/// Like [`analyze_source_with_timing`] but with an optional cross-file type
/// registry (built from the analyzed file's crate) merged under the file's own
/// definitions. The file-based entry points pass `Some(..)`; pure-source and
/// test callers pass `None` (single-file behavior, unchanged).
fn analyze_source_with_timing_defs(
    source: &str,
    function_name: Option<&str>,
    mut timing: Option<&mut TimingCollector>,
    extra_defs: Option<&CrateTypeRegistry>,
) -> Result<Vec<FunctionAnalysis>, AnalyzeError> {
    let file = if let Some(timing) = timing.as_deref_mut() {
        timing.record("analyze.parse", |_| {
            syn::parse_file(source).map_err(|e| AnalyzeError::ParseError(e.to_string()))
        })?
    } else {
        syn::parse_file(source).map_err(|e| AnalyzeError::ParseError(e.to_string()))?
    };

    let results = if let Some(timing) = timing.as_mut() {
        timing.record("analyze.walk", |_| {
            let (structs, enums) = merge_type_defs(extra_defs, &file);
            let mut results = Vec::new();

            for item in &file.items {
                if let syn::Item::Fn(item_fn) = item {
                    let name = item_fn.sig.ident.to_string();

                    if let Some(target) = function_name
                        && name != target
                    {
                        continue;
                    }

                    results.push(analyze_function(item_fn, &structs, &enums));
                }
            }

            results
        })
    } else {
        let (structs, enums) = merge_type_defs(extra_defs, &file);
        let mut results = Vec::new();

        for item in &file.items {
            if let syn::Item::Fn(item_fn) = item {
                let name = item_fn.sig.ident.to_string();

                if let Some(target) = function_name
                    && name != target
                {
                    continue;
                }

                results.push(analyze_function(item_fn, &structs, &enums));
            }
        }

        results
    };

    if function_name.is_some() && results.is_empty() {
        return Err(AnalyzeError::FunctionNotFound(
            function_name.unwrap_or_default().to_string(),
        ));
    }

    Ok(results)
}

// ─── Context-returning variants ─────────────────────────────────────────────

/// Analyze a Rust file and return both function analyses and file-level context.
pub fn analyze_file_with_context(
    file_path: &Path,
    function_name: Option<&str>,
) -> Result<(Vec<FunctionAnalysis>, FileContext), AnalyzeError> {
    analyze_file_with_context_and_timing(file_path, function_name, None)
}

/// Analyze a Rust file with timing, returning both function analyses and
/// file-level context.
pub fn analyze_file_with_context_and_timing(
    file_path: &Path,
    function_name: Option<&str>,
    mut timing: Option<&mut TimingCollector>,
) -> Result<(Vec<FunctionAnalysis>, FileContext), AnalyzeError> {
    if !file_path.exists() {
        return Err(AnalyzeError::FileNotFound(file_path.display().to_string()));
    }

    let source = if let Some(timing) = timing.as_deref_mut() {
        timing.record("analyze.read", |_| {
            std::fs::read_to_string(file_path).map_err(|e| AnalyzeError::ReadError(e.to_string()))
        })?
    } else {
        std::fs::read_to_string(file_path).map_err(|e| AnalyzeError::ReadError(e.to_string()))?
    };

    let registry = build_crate_type_registry(file_path);
    analyze_source_with_context_and_timing_defs(&source, function_name, timing, Some(&registry))
}

/// Analyze Rust source code from a string, returning both function analyses
/// and file-level context.
pub fn analyze_source_with_context(
    source: &str,
    function_name: Option<&str>,
) -> Result<(Vec<FunctionAnalysis>, FileContext), AnalyzeError> {
    analyze_source_with_context_and_timing_defs(source, function_name, None, None)
}

fn analyze_source_with_context_and_timing_defs(
    source: &str,
    function_name: Option<&str>,
    mut timing: Option<&mut TimingCollector>,
    extra_defs: Option<&CrateTypeRegistry>,
) -> Result<(Vec<FunctionAnalysis>, FileContext), AnalyzeError> {
    let file = if let Some(timing) = timing.as_deref_mut() {
        timing.record("analyze.parse", |_| {
            syn::parse_file(source).map_err(|e| AnalyzeError::ParseError(e.to_string()))
        })?
    } else {
        syn::parse_file(source).map_err(|e| AnalyzeError::ParseError(e.to_string()))?
    };

    let file_ctx = collect_file_context(&file);

    let results = if let Some(timing) = timing.as_mut() {
        timing.record("analyze.walk", |_| {
            let (structs, enums) = merge_type_defs(extra_defs, &file);
            let mut results = Vec::new();

            for item in &file.items {
                if let syn::Item::Fn(item_fn) = item {
                    let name = item_fn.sig.ident.to_string();
                    if let Some(target) = function_name
                        && name != target
                    {
                        continue;
                    }
                    results.push(analyze_function(item_fn, &structs, &enums));
                }
            }

            results
        })
    } else {
        let (structs, enums) = merge_type_defs(extra_defs, &file);
        let mut results = Vec::new();

        for item in &file.items {
            if let syn::Item::Fn(item_fn) = item {
                let name = item_fn.sig.ident.to_string();
                if let Some(target) = function_name
                    && name != target
                {
                    continue;
                }
                results.push(analyze_function(item_fn, &structs, &enums));
            }
        }

        results
    };

    if function_name.is_some() && results.is_empty() {
        return Err(AnalyzeError::FunctionNotFound(
            function_name.unwrap_or_default().to_string(),
        ));
    }

    Ok((results, file_ctx))
}

// ─── File Context Extraction ────────────────────────────────────────────────

/// Extract file-level context (use paths, tokio macros) from a parsed
/// `syn::File` for use by adapter recognizers.
pub fn collect_file_context(file: &syn::File) -> FileContext {
    let mut use_paths = Vec::new();
    let mut has_tokio_macro = false;

    for item in &file.items {
        match item {
            syn::Item::Use(item_use) => {
                collect_use_paths(&item_use.tree, String::new(), &mut use_paths);
            }
            syn::Item::Fn(item_fn) if !has_tokio_macro => {
                for attr in &item_fn.attrs {
                    if is_tokio_macro_attr(attr) {
                        has_tokio_macro = true;
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    FileContext {
        use_paths,
        has_tokio_macro,
    }
}

fn collect_use_paths(tree: &syn::UseTree, prefix: String, out: &mut Vec<String>) {
    match tree {
        syn::UseTree::Path(p) => {
            let new_prefix = if prefix.is_empty() {
                p.ident.to_string()
            } else {
                format!("{}::{}", prefix, p.ident)
            };
            collect_use_paths(&p.tree, new_prefix, out);
        }
        syn::UseTree::Name(n) => {
            let full = if prefix.is_empty() {
                n.ident.to_string()
            } else {
                format!("{}::{}", prefix, n.ident)
            };
            out.push(full);
        }
        syn::UseTree::Glob(_) => {
            let full = if prefix.is_empty() {
                "*".to_string()
            } else {
                format!("{}::*", prefix)
            };
            out.push(full);
        }
        syn::UseTree::Group(g) => {
            for item in &g.items {
                collect_use_paths(item, prefix.clone(), out);
            }
        }
        syn::UseTree::Rename(r) => {
            let full = if prefix.is_empty() {
                r.ident.to_string()
            } else {
                format!("{}::{}", prefix, r.ident)
            };
            out.push(full);
        }
    }
}

fn is_tokio_macro_attr(attr: &syn::Attribute) -> bool {
    let path = match &attr.meta {
        syn::Meta::Path(p) => p,
        syn::Meta::List(list) => &list.path,
        syn::Meta::NameValue(_) => return false,
    };
    let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
    segments == ["tokio", "main"] || segments == ["tokio", "test"]
}

/// Collected struct definitions from the same file.
type StructDefs = HashMap<String, Vec<(String, syn::Type)>>;

/// Collected enum definitions: maps enum name → Vec of (variant_name, fields).
type EnumDefs = HashMap<String, Vec<(String, syn::Fields)>>;

/// Read a `#[serde(rename_all = "…")]` value from a set of attributes, if present.
fn serde_rename_all(attrs: &[syn::Attribute]) -> Option<String> {
    serde_string_arg(attrs, "rename_all")
}

/// Read a field-level `#[serde(rename = "…")]` value, if present.
fn serde_field_rename(attrs: &[syn::Attribute]) -> Option<String> {
    serde_string_arg(attrs, "rename")
}

/// Extract the string value of a `#[serde(<key> = "…")]` argument.
fn serde_string_arg(attrs: &[syn::Attribute], key: &str) -> Option<String> {
    for attr in attrs {
        if !attr.path().is_ident("serde") {
            continue;
        }
        let mut found = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident(key) {
                let value = meta.value()?;
                let lit: syn::LitStr = value.parse()?;
                found = Some(lit.value());
            }
            Ok(())
        });
        if found.is_some() {
            return found;
        }
    }
    None
}

fn capitalize_word(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// Apply a serde `rename_all` rule to a (snake_case) Rust field name, producing
/// the JSON key serde would expect. Unknown rules leave the name unchanged.
fn apply_rename_all(field: &str, rule: &str) -> String {
    let words: Vec<&str> = field.split('_').filter(|w| !w.is_empty()).collect();
    match rule {
        "lowercase" => words.concat().to_lowercase(),
        "UPPERCASE" => words.concat().to_uppercase(),
        "PascalCase" => words.iter().map(|w| capitalize_word(w)).collect(),
        "camelCase" => words
            .iter()
            .enumerate()
            .map(|(i, w)| {
                if i == 0 {
                    w.to_lowercase()
                } else {
                    capitalize_word(w)
                }
            })
            .collect(),
        "snake_case" => words.join("_").to_lowercase(),
        "SCREAMING_SNAKE_CASE" => words.join("_").to_uppercase(),
        "kebab-case" => words.join("-").to_lowercase(),
        "SCREAMING-KEBAB-CASE" => words.join("-").to_uppercase(),
        _ => field.to_string(),
    }
}

/// Resolve the JSON key for a struct field, honoring serde `rename`/`rename_all`
/// (str-55ep). Without this, synthesized struct inputs use raw snake_case Rust
/// field names and fail to deserialize into structs declaring a different
/// `rename_all` (e.g. `camelCase`).
fn field_json_key(field: &syn::Field, rename_all: Option<&str>) -> String {
    let raw = field
        .ident
        .as_ref()
        .map(|id| id.to_string())
        .unwrap_or_default();
    serde_field_rename(&field.attrs)
        .or_else(|| rename_all.map(|rule| apply_rename_all(&raw, rule)))
        .unwrap_or(raw)
}

fn collect_struct_defs(file: &syn::File) -> StructDefs {
    let mut defs = HashMap::new();
    for item in &file.items {
        if let syn::Item::Struct(s) = item
            && let syn::Fields::Named(fields) = &s.fields
        {
            let rename_all = serde_rename_all(&s.attrs);
            let field_list: Vec<(String, syn::Type)> = fields
                .named
                .iter()
                .filter(|f| f.ident.is_some())
                .map(|f| (field_json_key(f, rename_all.as_deref()), f.ty.clone()))
                .collect();
            defs.insert(s.ident.to_string(), field_list);
        }
    }
    defs
}

fn collect_enum_defs(file: &syn::File) -> EnumDefs {
    let mut defs = HashMap::new();
    for item in &file.items {
        if let syn::Item::Enum(e) = item {
            let variants: Vec<(String, syn::Fields)> = e
                .variants
                .iter()
                .map(|v| (v.ident.to_string(), v.fields.clone()))
                .collect();
            defs.insert(e.ident.to_string(), variants);
        }
    }
    defs
}

/// Cross-file (same-crate) struct/enum definitions, keyed by bare type name.
///
/// Built once per file-based analyze call by walking every `.rs` file in the
/// crate that contains the analyzed file. This lifts the historical single-file
/// limitation: a function taking a type defined in another module of the same
/// crate (e.g. `use crate::domain::Trip`) can now be synthesized instead of
/// classified `Opaque` and skipped. See str-do53.
///
/// Resolution is by bare name (the last path segment), matching how
/// `convert_type_path` already keys lookups, so `crate::domain::Trip`,
/// `domain::Trip`, and `Trip` all resolve identically. Bare names defined in
/// more than one file are AMBIGUOUS and are dropped from the registry — they
/// stay `Opaque` rather than risk synthesizing the wrong type. Definitions in
/// the file being analyzed always take precedence over the registry.
type CrateTypeRegistry = (StructDefs, EnumDefs);

/// Find the crate root for `file_path`: the nearest ancestor directory that
/// contains a `Cargo.toml`. Returns `None` if none is found (e.g. a bare file
/// outside any crate), in which case cross-file resolution is skipped.
fn find_crate_root(file_path: &Path) -> Option<PathBuf> {
    let mut dir = file_path.parent();
    while let Some(d) = dir {
        if d.join("Cargo.toml").is_file() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// Maximum directory recursion depth when walking a crate for source files.
/// A cheap backstop against pathological trees; real crate layouts are shallow.
const MAX_CRATE_WALK_DEPTH: usize = 64;

/// Recursively collect `.rs` files under `dir`, skipping `target/` and hidden
/// directories. Symlinked directories are NOT followed (uses `file_type()`,
/// which does not traverse symlinks) so a symlink loop cannot cause unbounded
/// recursion; a depth cap is a further backstop. Bounded to keep the per-call
/// cost proportional to crate size.
fn collect_rust_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth >= MAX_CRATE_WALK_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        // `file_type()` from the dir entry does not follow symlinks, so a
        // symlinked directory reports as a symlink (not a dir) and is skipped.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if file_type.is_dir() {
            if name == "target" || name.starts_with('.') {
                continue;
            }
            collect_rust_files(&path, depth + 1, out);
        } else if file_type.is_file() && name.ends_with(".rs") {
            out.push(path);
        }
    }
}

/// Build the cross-file type registry for the crate containing `file_path`.
///
/// Walks the crate's `src/` (falling back to the crate root) and collects all
/// struct/enum definitions. Bare names that appear in more than one file are
/// dropped so ambiguous types stay `Opaque`. Returns empty maps when no crate
/// root is found or the crate has no parseable sources.
fn build_crate_type_registry(file_path: &Path) -> CrateTypeRegistry {
    let Some(crate_root) = find_crate_root(file_path) else {
        return (HashMap::new(), HashMap::new());
    };
    let src = crate_root.join("src");
    let scan_root = if src.is_dir() { src } else { crate_root };

    let mut files = Vec::new();
    collect_rust_files(&scan_root, 0, &mut files);

    let mut structs: StructDefs = HashMap::new();
    let mut enums: EnumDefs = HashMap::new();
    let mut dupes: HashSet<String> = HashSet::new();

    for rs in files {
        let Ok(source) = std::fs::read_to_string(&rs) else {
            continue;
        };
        let Ok(parsed) = syn::parse_file(&source) else {
            continue;
        };
        for (name, fields) in collect_struct_defs(&parsed) {
            if structs.insert(name.clone(), fields).is_some() {
                dupes.insert(name);
            }
        }
        for (name, variants) in collect_enum_defs(&parsed) {
            if enums.insert(name.clone(), variants).is_some() {
                dupes.insert(name);
            }
        }
    }

    // Drop any name that is ambiguous — defined more than once, whether across
    // files or across the struct/enum kinds (a `struct Widget` in one file and
    // an `enum Widget` in another must not silently resolve to the struct).
    for name in structs.keys().filter(|n| enums.contains_key(*n)) {
        dupes.insert(name.clone());
    }
    for name in dupes {
        structs.remove(&name);
        enums.remove(&name);
    }

    (structs, enums)
}

/// Merge the cross-file registry under the definitions found in the analyzed
/// file. The current file always wins so a local type shadows a same-named type
/// elsewhere in the crate.
fn merge_type_defs(extra: Option<&CrateTypeRegistry>, file: &syn::File) -> CrateTypeRegistry {
    let (mut structs, mut enums) = match extra {
        Some((s, e)) => (s.clone(), e.clone()),
        None => (HashMap::new(), HashMap::new()),
    };
    for (name, fields) in collect_struct_defs(file) {
        structs.insert(name, fields);
    }
    for (name, variants) in collect_enum_defs(file) {
        enums.insert(name, variants);
    }
    (structs, enums)
}

/// Convert an enum variant's fields to a TypeInfo.
fn enum_variant_to_type(
    fields: &syn::Fields,
    structs: &StructDefs,
    enums: &EnumDefs,
    generic_params: &HashSet<String>,
    converting: &mut HashSet<String>,
) -> TypeInfo {
    match fields {
        syn::Fields::Unit => TypeInfo::Unknown,
        syn::Fields::Unnamed(f) if f.unnamed.len() == 1 => {
            convert_type_inner(&f.unnamed[0].ty, structs, enums, generic_params, converting)
        }
        syn::Fields::Unnamed(f) => {
            let flds = f
                .unnamed
                .iter()
                .enumerate()
                .map(|(i, field)| {
                    (
                        i.to_string(),
                        convert_type_inner(&field.ty, structs, enums, generic_params, converting),
                    )
                })
                .collect();
            TypeInfo::Object { fields: flds }
        }
        syn::Fields::Named(f) => {
            let flds = f
                .named
                .iter()
                .filter_map(|field| {
                    field.ident.as_ref().map(|id| {
                        (
                            id.to_string(),
                            convert_type_inner(
                                &field.ty,
                                structs,
                                enums,
                                generic_params,
                                converting,
                            ),
                        )
                    })
                })
                .collect();
            TypeInfo::Object { fields: flds }
        }
    }
}

fn analyze_function(
    item_fn: &syn::ItemFn,
    structs: &StructDefs,
    enums: &EnumDefs,
) -> FunctionAnalysis {
    let name = item_fn.sig.ident.to_string();
    let exported = matches!(item_fn.vis, syn::Visibility::Public(_));

    // Collect generic type parameter names so T maps to Unknown, not Opaque.
    let generic_params: HashSet<String> = item_fn
        .sig
        .generics
        .params
        .iter()
        .filter_map(|p| {
            if let syn::GenericParam::Type(tp) = p {
                Some(tp.ident.to_string())
            } else {
                None
            }
        })
        .collect();

    let params = extract_params(&item_fn.sig.inputs, structs, enums, &generic_params);
    let param_names: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();

    let return_type = convert_return_type(&item_fn.sig.output, structs, enums, &generic_params);

    let start_line = item_fn.sig.ident.span().start().line as u32;
    let end_line = item_fn.block.brace_token.span.close().end().line as u32;

    let branches = extract_branches(&item_fn.block, &param_names);
    let dependencies = extract_dependencies(&item_fn.block, &param_names);
    let literals = extract_literals(&item_fn.block);

    let is_async = item_fn.sig.asyncness.is_some();

    FunctionAnalysis {
        name,
        exported,
        params,
        branches,
        dependencies,
        return_type,
        start_line,
        end_line,
        literals,
        crypto_boundaries: vec![],
        loops: vec![],
        source_file: None,
        is_async,
        adapter_hints: vec![],
        invocation_model: InvocationModel::default(),
    }
}

// ─── Parameter & Type Extraction ─────────────────────────────────────────────

fn extract_params(
    inputs: &syn::punctuated::Punctuated<syn::FnArg, syn::token::Comma>,
    structs: &StructDefs,
    enums: &EnumDefs,
    generic_params: &HashSet<String>,
) -> Vec<ParamInfo> {
    let mut params = Vec::new();
    for arg in inputs {
        if let syn::FnArg::Typed(pat_type) = arg {
            let name = extract_pat_name(&pat_type.pat);
            let type_name = extract_type_name(&pat_type.ty);
            let typ = convert_type(&pat_type.ty, structs, enums, generic_params);
            params.push(ParamInfo {
                name,
                typ,
                type_name,
            });
        }
        // Skip FnArg::Receiver (self params)
    }
    params
}

fn extract_pat_name(pat: &syn::Pat) -> String {
    match pat {
        syn::Pat::Ident(pi) => pi.ident.to_string(),
        syn::Pat::Wild(_) => "_".to_string(),
        syn::Pat::Reference(pr) => extract_pat_name(&pr.pat),
        _ => "_".to_string(),
    }
}

fn extract_type_name(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(tp) => {
            let seg = tp.path.segments.last()?;
            let name = seg.ident.to_string();
            // Only set type_name for non-primitive, non-standard-library types
            if is_primitive_name(&name) || is_well_known_generic(&name) {
                None
            } else {
                Some(name)
            }
        }
        syn::Type::Reference(tr) => extract_type_name(&tr.elem),
        _ => None,
    }
}

fn is_primitive_name(name: &str) -> bool {
    matches!(
        name,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
            | "bool"
            | "String"
            | "str"
            | "char"
    )
}

fn is_well_known_generic(name: &str) -> bool {
    matches!(
        name,
        "Vec"
            | "Option"
            | "Result"
            | "Box"
            | "Arc"
            | "Rc"
            | "HashMap"
            | "HashSet"
            | "BTreeMap"
            | "BTreeSet"
    )
}

fn convert_type(
    ty: &syn::Type,
    structs: &StructDefs,
    enums: &EnumDefs,
    generic_params: &HashSet<String>,
) -> TypeInfo {
    convert_type_inner(ty, structs, enums, generic_params, &mut HashSet::new())
}

/// Inner type conversion with a `converting` set to guard against recursive enums.
fn convert_type_inner(
    ty: &syn::Type,
    structs: &StructDefs,
    enums: &EnumDefs,
    generic_params: &HashSet<String>,
    converting: &mut HashSet<String>,
) -> TypeInfo {
    match ty {
        syn::Type::Path(type_path) => {
            convert_type_path(type_path, structs, enums, generic_params, converting)
        }
        syn::Type::Reference(type_ref) => {
            convert_reference(type_ref, structs, enums, generic_params, converting)
        }
        syn::Type::Tuple(type_tuple) => {
            convert_tuple(type_tuple, structs, enums, generic_params, converting)
        }
        syn::Type::Array(type_array) => TypeInfo::Array {
            element: Box::new(convert_type_inner(
                &type_array.elem,
                structs,
                enums,
                generic_params,
                converting,
            )),
        },
        syn::Type::Slice(type_slice) => TypeInfo::Array {
            element: Box::new(convert_type_inner(
                &type_slice.elem,
                structs,
                enums,
                generic_params,
                converting,
            )),
        },
        syn::Type::Paren(type_paren) => {
            convert_type_inner(&type_paren.elem, structs, enums, generic_params, converting)
        }
        syn::Type::TraitObject(trait_obj) => {
            let label = trait_obj
                .bounds
                .iter()
                .filter_map(|b| {
                    if let syn::TypeParamBound::Trait(t) = b {
                        Some(
                            t.path
                                .segments
                                .iter()
                                .map(|s| s.ident.to_string())
                                .collect::<Vec<_>>()
                                .join("::"),
                        )
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" + ");
            TypeInfo::Opaque {
                label: format!("dyn {label}"),
            }
        }
        syn::Type::BareFn(_) => TypeInfo::Complex {
            kind: ComplexKind::Closure,
            metadata: HashMap::new(),
            inner: None,
        },
        syn::Type::Never(_) => TypeInfo::Unknown,
        _ => TypeInfo::Unknown,
    }
}

/// Construct a sized `TypeInfo::Int` carrying bit-width and signedness.
fn int_type(width: u8, signed: bool) -> TypeInfo {
    TypeInfo::Int {
        int_width: Some(width),
        int_signed: Some(signed),
    }
}

fn convert_type_path(
    type_path: &syn::TypePath,
    structs: &StructDefs,
    enums: &EnumDefs,
    generic_params: &HashSet<String>,
    converting: &mut HashSet<String>,
) -> TypeInfo {
    let Some(seg) = type_path.path.segments.last() else {
        return TypeInfo::Unknown;
    };
    let name = seg.ident.to_string();

    match name.as_str() {
        // Integer types — carry width/signedness so the core input generator can
        // constrain generated values to the type's range (str-ddxe). Without this
        // every int became a bare `Int` and the generator produced full-i64-range
        // values that failed to deserialize into u8/narrow/unsigned fields.
        "u8" => int_type(8, false),
        "i8" => int_type(8, true),
        "u16" => int_type(16, false),
        "i16" => int_type(16, true),
        "u32" => int_type(32, false),
        "i32" => int_type(32, true),
        "u64" => int_type(64, false),
        "i64" => int_type(64, true),
        "u128" => int_type(128, false),
        "i128" => int_type(128, true),
        "usize" => int_type(64, false),
        "isize" => int_type(64, true),

        // Float types
        "f32" | "f64" => TypeInfo::Float,

        // String types
        "String" => TypeInfo::Str,

        // Boolean
        "bool" => TypeInfo::Bool,

        // Character
        "char" => TypeInfo::Complex {
            kind: ComplexKind::Char,
            metadata: HashMap::new(),
            inner: None,
        },

        // Vec<T> → Array
        "Vec" => {
            let inner = extract_first_generic_arg(seg, structs, enums, generic_params, converting);
            TypeInfo::Array {
                element: Box::new(inner),
            }
        }

        // Option<T> → Nullable
        "Option" => {
            let inner = extract_first_generic_arg(seg, structs, enums, generic_params, converting);
            TypeInfo::Nullable {
                inner: Box::new(inner),
            }
        }

        // Result<T, E> → Union
        "Result" => {
            let variants = extract_generic_args(seg, structs, enums, generic_params, converting);
            TypeInfo::Union {
                variants,
                enum_values: Vec::new(),
            }
        }

        // Smart pointers / wrappers → unwrap to inner
        "Box" | "Arc" | "Rc" | "Cow" | "Cell" | "RefCell" | "Mutex" | "RwLock" => {
            extract_first_generic_arg(seg, structs, enums, generic_params, converting)
        }

        // HashMap/BTreeMap → Object with key/value fields
        "HashMap" | "BTreeMap" => {
            let args = extract_generic_args(seg, structs, enums, generic_params, converting);
            let key_type = args.first().cloned().unwrap_or(TypeInfo::Unknown);
            let val_type = args.get(1).cloned().unwrap_or(TypeInfo::Unknown);
            TypeInfo::Object {
                fields: vec![
                    ("_key".to_string(), key_type),
                    ("_value".to_string(), val_type),
                ],
            }
        }

        // HashSet/BTreeSet → Array
        "HashSet" | "BTreeSet" => {
            let inner = extract_first_generic_arg(seg, structs, enums, generic_params, converting);
            TypeInfo::Array {
                element: Box::new(inner),
            }
        }

        // Well-known complex types
        "PathBuf" | "Path" => TypeInfo::Complex {
            kind: ComplexKind::Path,
            metadata: HashMap::new(),
            inner: None,
        },
        "Duration" => TypeInfo::Complex {
            kind: ComplexKind::Duration,
            metadata: HashMap::new(),
            inner: None,
        },
        "IpAddr" | "Ipv4Addr" | "Ipv6Addr" => TypeInfo::Complex {
            kind: ComplexKind::IpAddress,
            metadata: HashMap::new(),
            inner: None,
        },
        "Url" => TypeInfo::Complex {
            kind: ComplexKind::Url,
            metadata: HashMap::new(),
            inner: None,
        },
        "Uuid" => TypeInfo::Complex {
            kind: ComplexKind::Uuid,
            metadata: HashMap::new(),
            inner: None,
        },
        // chrono date/time types (str-8euf). NaiveDate is a calendar date;
        // NaiveDateTime and DateTime<Tz> carry a time component. The harness
        // materializes the date/date_time envelopes into the ISO strings these
        // types deserialize from.
        "NaiveDate" => TypeInfo::Complex {
            kind: ComplexKind::Date,
            metadata: HashMap::new(),
            inner: None,
        },
        "NaiveDateTime" | "DateTime" => TypeInfo::Complex {
            kind: ComplexKind::DateTime,
            metadata: HashMap::new(),
            inner: None,
        },
        "Regex" => TypeInfo::Complex {
            kind: ComplexKind::RegExp,
            metadata: HashMap::new(),
            inner: None,
        },

        _ => {
            // Generic type parameter → Unknown (not an external type)
            if generic_params.contains(&name) {
                return TypeInfo::Unknown;
            }
            // Struct defined in this file or elsewhere in the crate → Object
            if let Some(fields) = structs.get(&name) {
                // Guard against self-referential / mutually-recursive structs,
                // whose blast radius widens once cross-file types are resolved.
                if !converting.insert(name.clone()) {
                    return TypeInfo::Opaque { label: name };
                }
                let object = TypeInfo::Object {
                    fields: fields
                        .iter()
                        .map(|(n, t)| {
                            (
                                n.clone(),
                                convert_type_inner(t, structs, enums, generic_params, converting),
                            )
                        })
                        .collect(),
                };
                converting.remove(&name);
                object
            // Enum defined in this file or elsewhere in the crate → Union
            } else if let Some(variants) = enums.get(&name) {
                // Guard against recursive enums
                if !converting.insert(name.clone()) {
                    return TypeInfo::Opaque { label: name };
                }
                let variant_types = variants
                    .iter()
                    .map(|(_, fields)| {
                        enum_variant_to_type(fields, structs, enums, generic_params, converting)
                    })
                    .collect();
                converting.remove(&name);
                // str-pjlc1: fieldless-enum value-domain extraction (populating
                // enum_values here) is a tracked follow-up; plain type union for now.
                TypeInfo::Union {
                    variants: variant_types,
                    enum_values: Vec::new(),
                }
            } else if name == "Value" {
                // serde_json::Value (and other dynamic-JSON value types): ANY
                // JSON deserializes into it, so it is freely synthesizable.
                // Unknown lets input_gen emit an arbitrary value without making
                // the enclosing struct non-executable (str-orku). The cross-file
                // struct/enum registry is checked above, so a user-defined
                // `Value` type still resolves to its real shape.
                TypeInfo::Unknown
            } else if name == "Map" {
                // serde_json::Map<String, Value>: a JSON object. An empty object
                // always deserializes; precise key/value typing is unnecessary
                // for synthesis (and `serde(flatten)` fields accept any object).
                TypeInfo::Object { fields: Vec::new() }
            } else {
                TypeInfo::Opaque { label: name }
            }
        }
    }
}

fn convert_reference(
    type_ref: &syn::TypeReference,
    structs: &StructDefs,
    enums: &EnumDefs,
    generic_params: &HashSet<String>,
    converting: &mut HashSet<String>,
) -> TypeInfo {
    // Special case: &str → Str
    if let syn::Type::Path(tp) = type_ref.elem.as_ref()
        && let Some(seg) = tp.path.segments.last()
        && seg.ident == "str"
    {
        return TypeInfo::Str;
    }
    // Special case: &[T] → Array
    if let syn::Type::Slice(ts) = type_ref.elem.as_ref() {
        return TypeInfo::Array {
            element: Box::new(convert_type_inner(
                &ts.elem,
                structs,
                enums,
                generic_params,
                converting,
            )),
        };
    }
    // Otherwise unwrap the reference (strips &, &mut, and lifetime annotations)
    convert_type_inner(&type_ref.elem, structs, enums, generic_params, converting)
}

fn convert_tuple(
    type_tuple: &syn::TypeTuple,
    structs: &StructDefs,
    enums: &EnumDefs,
    generic_params: &HashSet<String>,
    converting: &mut HashSet<String>,
) -> TypeInfo {
    if type_tuple.elems.is_empty() {
        return TypeInfo::Unknown; // unit type ()
    }
    let fields = type_tuple
        .elems
        .iter()
        .enumerate()
        .map(|(i, t)| {
            (
                i.to_string(),
                convert_type_inner(t, structs, enums, generic_params, converting),
            )
        })
        .collect();
    TypeInfo::Object { fields }
}

fn extract_first_generic_arg(
    seg: &syn::PathSegment,
    structs: &StructDefs,
    enums: &EnumDefs,
    generic_params: &HashSet<String>,
    converting: &mut HashSet<String>,
) -> TypeInfo {
    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments
        && let Some(syn::GenericArgument::Type(ty)) = args.args.first()
    {
        return convert_type_inner(ty, structs, enums, generic_params, converting);
    }
    TypeInfo::Unknown
}

fn extract_generic_args(
    seg: &syn::PathSegment,
    structs: &StructDefs,
    enums: &EnumDefs,
    generic_params: &HashSet<String>,
    converting: &mut HashSet<String>,
) -> Vec<TypeInfo> {
    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
        return args
            .args
            .iter()
            .filter_map(|a| {
                if let syn::GenericArgument::Type(ty) = a {
                    Some(convert_type_inner(
                        ty,
                        structs,
                        enums,
                        generic_params,
                        converting,
                    ))
                } else {
                    None
                }
            })
            .collect();
    }
    vec![]
}

fn convert_return_type(
    output: &syn::ReturnType,
    structs: &StructDefs,
    enums: &EnumDefs,
    generic_params: &HashSet<String>,
) -> TypeInfo {
    match output {
        syn::ReturnType::Default => TypeInfo::Unknown, // unit return
        syn::ReturnType::Type(_, ty) => convert_type(ty, structs, enums, generic_params),
    }
}

// ─── Branch Extraction ───────────────────────────────────────────────────────

fn extract_branches(block: &syn::Block, param_names: &HashSet<String>) -> Vec<BranchInfo> {
    let mut branches = Vec::new();
    let mut next_id: u32 = 0;
    walk_block_for_branches(block, param_names, &mut branches, &mut next_id, false);
    branches
}

fn walk_block_for_branches(
    block: &syn::Block,
    param_names: &HashSet<String>,
    branches: &mut Vec<BranchInfo>,
    next_id: &mut u32,
    _is_else_if: bool,
) {
    for stmt in &block.stmts {
        walk_stmt_for_branches(stmt, param_names, branches, next_id);
    }
}

fn walk_stmt_for_branches(
    stmt: &syn::Stmt,
    param_names: &HashSet<String>,
    branches: &mut Vec<BranchInfo>,
    next_id: &mut u32,
) {
    match stmt {
        syn::Stmt::Expr(expr, _) => {
            walk_expr_for_branches(expr, param_names, branches, next_id, false);
        }
        syn::Stmt::Local(local) => {
            if let Some(init) = &local.init {
                walk_expr_for_branches(&init.expr, param_names, branches, next_id, false);
                if let Some((_, diverge)) = &init.diverge {
                    walk_expr_for_branches(diverge, param_names, branches, next_id, false);
                }
            }
        }
        syn::Stmt::Item(_) | syn::Stmt::Macro(_) => {}
    }
}

fn walk_expr_for_branches(
    expr: &syn::Expr,
    param_names: &HashSet<String>,
    branches: &mut Vec<BranchInfo>,
    next_id: &mut u32,
    is_else_if: bool,
) {
    match expr {
        syn::Expr::If(expr_if) => {
            let line = expr_if.if_token.span().start().line as u32;
            let condition_text = expr_if.cond.to_token_stream().to_string();
            let condition = build_sym_expr(&expr_if.cond, param_names);
            let branch_type = if is_else_if {
                BranchType::ElseIf
            } else {
                BranchType::If
            };

            branches.push(BranchInfo {
                id: *next_id,
                line,
                condition_text,
                condition: meaningful(condition),
                branch_type,
            });
            *next_id += 1;

            // Recurse into then block
            walk_block_for_branches(&expr_if.then_branch, param_names, branches, next_id, false);

            // Handle else branch
            if let Some((_, else_branch)) = &expr_if.else_branch {
                walk_expr_for_branches(else_branch, param_names, branches, next_id, true);
            }
        }

        syn::Expr::Match(expr_match) => {
            // Recurse into the scrutinee
            walk_expr_for_branches(&expr_match.expr, param_names, branches, next_id, false);

            for arm in &expr_match.arms {
                let line = arm.pat.to_token_stream().to_string();
                let span_line = arm.fat_arrow_token.span().start().line as u32;

                let condition = build_pattern_sym_expr(&arm.pat, &expr_match.expr, param_names);

                branches.push(BranchInfo {
                    id: *next_id,
                    line: span_line,
                    condition_text: line,
                    condition: meaningful(condition),
                    branch_type: BranchType::Switch,
                });
                *next_id += 1;

                // Recurse into arm body
                walk_expr_for_branches(&arm.body, param_names, branches, next_id, false);
            }
        }

        syn::Expr::While(expr_while) => {
            let line = expr_while.while_token.span().start().line as u32;
            let condition_text = expr_while.cond.to_token_stream().to_string();
            let condition = build_sym_expr(&expr_while.cond, param_names);

            branches.push(BranchInfo {
                id: *next_id,
                line,
                condition_text,
                condition: meaningful(condition),
                branch_type: BranchType::While,
            });
            *next_id += 1;

            walk_block_for_branches(&expr_while.body, param_names, branches, next_id, false);
        }

        syn::Expr::ForLoop(expr_for) => {
            let line = expr_for.for_token.span().start().line as u32;
            let condition_text = expr_for.expr.to_token_stream().to_string();

            branches.push(BranchInfo {
                id: *next_id,
                line,
                condition_text,
                condition: None, // for-in loops don't have boolean conditions
                branch_type: BranchType::For,
            });
            *next_id += 1;

            walk_block_for_branches(&expr_for.body, param_names, branches, next_id, false);
        }

        syn::Expr::Loop(expr_loop) => {
            let line = expr_loop.loop_token.span().start().line as u32;

            branches.push(BranchInfo {
                id: *next_id,
                line,
                condition_text: "loop".to_string(),
                condition: None,
                branch_type: BranchType::While,
            });
            *next_id += 1;

            walk_block_for_branches(&expr_loop.body, param_names, branches, next_id, false);
        }

        syn::Expr::Try(expr_try) => {
            let line = expr_try.question_token.span.start().line as u32;
            let condition_text = format!("{}?", expr_try.expr.to_token_stream());

            branches.push(BranchInfo {
                id: *next_id,
                line,
                condition_text,
                condition: None,
                branch_type: BranchType::If, // ? is an implicit if Ok/Err
            });
            *next_id += 1;

            walk_expr_for_branches(&expr_try.expr, param_names, branches, next_id, false);
        }

        syn::Expr::Block(expr_block) => {
            walk_block_for_branches(&expr_block.block, param_names, branches, next_id, false);
        }

        syn::Expr::Return(expr_ret) => {
            if let Some(e) = &expr_ret.expr {
                walk_expr_for_branches(e, param_names, branches, next_id, false);
            }
        }

        syn::Expr::Call(expr_call) => {
            walk_expr_for_branches(&expr_call.func, param_names, branches, next_id, false);
            for arg in &expr_call.args {
                walk_expr_for_branches(arg, param_names, branches, next_id, false);
            }
        }

        syn::Expr::MethodCall(expr_mc) => {
            walk_expr_for_branches(&expr_mc.receiver, param_names, branches, next_id, false);
            for arg in &expr_mc.args {
                walk_expr_for_branches(arg, param_names, branches, next_id, false);
            }
        }

        syn::Expr::Binary(expr_bin) => {
            // Check for short-circuit operators
            match &expr_bin.op {
                syn::BinOp::And(token) => {
                    let line = token.spans[0].start().line as u32;
                    let condition_text = expr.to_token_stream().to_string();
                    let condition = build_sym_expr(expr, param_names);

                    branches.push(BranchInfo {
                        id: *next_id,
                        line,
                        condition_text,
                        condition: meaningful(condition),
                        branch_type: BranchType::LogicalAnd,
                    });
                    *next_id += 1;
                }
                syn::BinOp::Or(token) => {
                    let line = token.spans[0].start().line as u32;
                    let condition_text = expr.to_token_stream().to_string();
                    let condition = build_sym_expr(expr, param_names);

                    branches.push(BranchInfo {
                        id: *next_id,
                        line,
                        condition_text,
                        condition: meaningful(condition),
                        branch_type: BranchType::LogicalOr,
                    });
                    *next_id += 1;
                }
                _ => {}
            }

            walk_expr_for_branches(&expr_bin.left, param_names, branches, next_id, false);
            walk_expr_for_branches(&expr_bin.right, param_names, branches, next_id, false);
        }

        syn::Expr::Let(expr_let) => {
            walk_expr_for_branches(&expr_let.expr, param_names, branches, next_id, false);
        }

        syn::Expr::Assign(expr_assign) => {
            walk_expr_for_branches(&expr_assign.right, param_names, branches, next_id, false);
        }

        syn::Expr::Closure(expr_closure) => {
            walk_expr_for_branches(&expr_closure.body, param_names, branches, next_id, false);
        }

        syn::Expr::Tuple(expr_tuple) => {
            for elem in &expr_tuple.elems {
                walk_expr_for_branches(elem, param_names, branches, next_id, false);
            }
        }

        syn::Expr::Unary(expr_unary) => {
            walk_expr_for_branches(&expr_unary.expr, param_names, branches, next_id, false);
        }

        syn::Expr::Paren(expr_paren) => {
            walk_expr_for_branches(&expr_paren.expr, param_names, branches, next_id, false);
        }

        syn::Expr::Reference(expr_ref) => {
            walk_expr_for_branches(&expr_ref.expr, param_names, branches, next_id, false);
        }

        syn::Expr::Field(expr_field) => {
            walk_expr_for_branches(&expr_field.base, param_names, branches, next_id, false);
        }

        syn::Expr::Index(expr_index) => {
            walk_expr_for_branches(&expr_index.expr, param_names, branches, next_id, false);
            walk_expr_for_branches(&expr_index.index, param_names, branches, next_id, false);
        }

        syn::Expr::Unsafe(expr_unsafe) => {
            walk_block_for_branches(&expr_unsafe.block, param_names, branches, next_id, false);
        }

        _ => {}
    }
}

// ─── Symbolic Expression Building ────────────────────────────────────────────

fn build_sym_expr(expr: &syn::Expr, param_names: &HashSet<String>) -> SymExpr {
    match expr {
        syn::Expr::Path(expr_path) => {
            if let Some(ident) = expr_path.path.get_ident() {
                let name = ident.to_string();
                if param_names.contains(&name) {
                    return SymExpr::Param { name, path: vec![] };
                }
            }
            SymExpr::Unknown
        }

        syn::Expr::Field(expr_field) => {
            // resolve_field_chain already returns the path root-first
            // (`w.dims.height` -> ["dims", "height"]): it pushes each segment
            // AFTER recursing into the base. Reversing here produced leaf-first
            // paths for nested chains (`w.height.dims`), diverging from the
            // instrumentor's field-chain lowering and the solver/orchestrator
            // consumers, which all expect root-first (str-do53 review).
            if let Some((base_name, path)) = resolve_field_chain(expr, param_names) {
                return SymExpr::Param {
                    name: base_name,
                    path,
                };
            }
            let _ = expr_field;
            SymExpr::Unknown
        }

        syn::Expr::Binary(expr_bin) => {
            let Some(op) = convert_bin_op(&expr_bin.op) else {
                return SymExpr::Unknown;
            };
            let left = build_sym_expr(&expr_bin.left, param_names);
            let right = build_sym_expr(&expr_bin.right, param_names);
            SymExpr::BinOp {
                op,
                left: Box::new(left),
                right: Box::new(right),
            }
        }

        syn::Expr::Unary(expr_unary) => {
            let op = match &expr_unary.op {
                syn::UnOp::Not(_) => UnOpKind::Not,
                syn::UnOp::Neg(_) => UnOpKind::Neg,
                _ => return SymExpr::Unknown,
            };
            let operand = build_sym_expr(&expr_unary.expr, param_names);
            SymExpr::UnOp {
                op,
                operand: Box::new(operand),
            }
        }

        syn::Expr::Lit(expr_lit) => {
            let cv = convert_lit(&expr_lit.lit);
            SymExpr::Const(cv)
        }

        syn::Expr::MethodCall(mc) => {
            let name = mc.method.to_string();
            let receiver = build_sym_expr(&mc.receiver, param_names);
            let args: Vec<SymExpr> = mc
                .args
                .iter()
                .map(|a| build_sym_expr(a, param_names))
                .collect();
            SymExpr::Call {
                name,
                receiver: Some(Box::new(receiver)),
                args,
            }
        }

        syn::Expr::Call(call) => {
            let name = call.func.to_token_stream().to_string();
            let args: Vec<SymExpr> = call
                .args
                .iter()
                .map(|a| build_sym_expr(a, param_names))
                .collect();
            SymExpr::Call {
                name,
                receiver: None,
                args,
            }
        }

        syn::Expr::Paren(paren) => build_sym_expr(&paren.expr, param_names),

        syn::Expr::Reference(reference) => build_sym_expr(&reference.expr, param_names),

        syn::Expr::Let(expr_let) => {
            // `let Some(x) = expr` in if-let: build condition from the pattern.
            build_pattern_sym_expr(&expr_let.pat, &expr_let.expr, param_names)
        }

        _ => SymExpr::Unknown,
    }
}

fn resolve_field_chain(
    expr: &syn::Expr,
    param_names: &HashSet<String>,
) -> Option<(String, Vec<String>)> {
    match expr {
        syn::Expr::Field(field) => {
            let field_name = match &field.member {
                syn::Member::Named(ident) => ident.to_string(),
                syn::Member::Unnamed(idx) => idx.index.to_string(),
            };
            if let Some((base, mut path)) = resolve_field_chain(&field.base, param_names) {
                path.push(field_name);
                Some((base, path))
            } else {
                None
            }
        }
        syn::Expr::Path(path) => {
            if let Some(ident) = path.path.get_ident() {
                let name = ident.to_string();
                if param_names.contains(&name) {
                    return Some((name, vec![]));
                }
            }
            None
        }
        syn::Expr::Reference(reference) => resolve_field_chain(&reference.expr, param_names),
        _ => None,
    }
}

fn build_pattern_sym_expr(
    pat: &syn::Pat,
    scrutinee: &syn::Expr,
    param_names: &HashSet<String>,
) -> SymExpr {
    match pat {
        syn::Pat::Const(pat_const) => {
            let left = build_sym_expr(scrutinee, param_names);
            let right = build_sym_expr(
                &pat_const
                    .block
                    .stmts
                    .first()
                    .and_then(|s| {
                        if let syn::Stmt::Expr(e, _) = s {
                            Some(e)
                        } else {
                            None
                        }
                    })
                    .cloned()
                    .unwrap_or_else(|| syn::parse_quote!(())),
                param_names,
            );
            SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(left),
                right: Box::new(right),
            }
        }

        syn::Pat::Lit(pat_lit) => {
            let left = build_sym_expr(scrutinee, param_names);
            let right = SymExpr::Const(convert_lit(&pat_lit.lit));
            SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(left),
                right: Box::new(right),
            }
        }

        syn::Pat::Ident(pat_ident) => {
            // Bare identifier — unit enum variant or variable binding.
            let left = build_sym_expr(scrutinee, param_names);
            let right = SymExpr::Const(ConstValue::Str(pat_ident.ident.to_string()));
            SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(left),
                right: Box::new(right),
            }
        }

        syn::Pat::TupleStruct(pat_ts) => {
            // `Some(x)`, `Ok(v)`, `Err(e)` — variant discriminant condition.
            let left = build_sym_expr(scrutinee, param_names);
            let variant_name = pat_ts
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();
            let right = SymExpr::Const(ConstValue::Str(variant_name));
            SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(left),
                right: Box::new(right),
            }
        }

        syn::Pat::Struct(pat_struct) => {
            // `Point { x, y }` or `Variant { field, .. }` — struct/variant discriminant.
            let left = build_sym_expr(scrutinee, param_names);
            let variant_name = pat_struct
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();
            let right = SymExpr::Const(ConstValue::Str(variant_name));
            SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(left),
                right: Box::new(right),
            }
        }

        syn::Pat::Path(pat_path) => {
            // Qualified path like `Status::Active` in match arms.
            let left = build_sym_expr(scrutinee, param_names);
            let variant_name = pat_path
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();
            let right = SymExpr::Const(ConstValue::Str(variant_name));
            SymExpr::BinOp {
                op: BinOpKind::Eq,
                left: Box::new(left),
                right: Box::new(right),
            }
        }

        syn::Pat::Or(pat_or) => {
            // `1 | 2 | 3` — build left-folded Or tree.
            let mut cases = pat_or.cases.iter();
            let first = cases
                .next()
                .map(|p| build_pattern_sym_expr(p, scrutinee, param_names))
                .unwrap_or(SymExpr::Unknown);
            cases.fold(first, |acc, p| {
                let rhs = build_pattern_sym_expr(p, scrutinee, param_names);
                SymExpr::BinOp {
                    op: BinOpKind::Or,
                    left: Box::new(acc),
                    right: Box::new(rhs),
                }
            })
        }

        syn::Pat::Range(pat_range) => {
            // `1..=10`, `..5`, `3..` — build conjunction of bounds.
            let subj = build_sym_expr(scrutinee, param_names);
            let lo_expr = pat_range
                .start
                .as_deref()
                .map(|e| build_sym_expr(e, param_names));
            let hi_expr = pat_range
                .end
                .as_deref()
                .map(|e| build_sym_expr(e, param_names));
            let hi_op = match &pat_range.limits {
                syn::RangeLimits::HalfOpen(_) => BinOpKind::Lt,
                syn::RangeLimits::Closed(_) => BinOpKind::Le,
            };
            match (lo_expr, hi_expr) {
                (Some(lo), Some(hi)) => {
                    let ge = SymExpr::BinOp {
                        op: BinOpKind::Ge,
                        left: Box::new(subj.clone()),
                        right: Box::new(lo),
                    };
                    let le = SymExpr::BinOp {
                        op: hi_op,
                        left: Box::new(subj),
                        right: Box::new(hi),
                    };
                    SymExpr::BinOp {
                        op: BinOpKind::And,
                        left: Box::new(ge),
                        right: Box::new(le),
                    }
                }
                (Some(lo), None) => SymExpr::BinOp {
                    op: BinOpKind::Ge,
                    left: Box::new(subj),
                    right: Box::new(lo),
                },
                (None, Some(hi)) => SymExpr::BinOp {
                    op: hi_op,
                    left: Box::new(subj),
                    right: Box::new(hi),
                },
                (None, None) => SymExpr::Unknown,
            }
        }

        syn::Pat::Wild(_) => SymExpr::Unknown,
        _ => SymExpr::Unknown,
    }
}

fn convert_bin_op(op: &syn::BinOp) -> Option<BinOpKind> {
    Some(match op {
        syn::BinOp::Eq(_) => BinOpKind::Eq,
        syn::BinOp::Ne(_) => BinOpKind::Ne,
        syn::BinOp::Lt(_) => BinOpKind::Lt,
        syn::BinOp::Le(_) => BinOpKind::Le,
        syn::BinOp::Gt(_) => BinOpKind::Gt,
        syn::BinOp::Ge(_) => BinOpKind::Ge,
        syn::BinOp::Add(_) | syn::BinOp::AddAssign(_) => BinOpKind::Add,
        syn::BinOp::Sub(_) | syn::BinOp::SubAssign(_) => BinOpKind::Sub,
        syn::BinOp::Mul(_) | syn::BinOp::MulAssign(_) => BinOpKind::Mul,
        syn::BinOp::Div(_) | syn::BinOp::DivAssign(_) => BinOpKind::Div,
        syn::BinOp::Rem(_) | syn::BinOp::RemAssign(_) => BinOpKind::Mod,
        syn::BinOp::And(_) => BinOpKind::And,
        syn::BinOp::Or(_) => BinOpKind::Or,
        syn::BinOp::BitAnd(_) | syn::BinOp::BitAndAssign(_) => BinOpKind::BitwiseAnd,
        syn::BinOp::BitOr(_) | syn::BinOp::BitOrAssign(_) => BinOpKind::BitwiseOr,
        syn::BinOp::BitXor(_) | syn::BinOp::BitXorAssign(_) => BinOpKind::BitwiseXor,
        _ => return None,
    })
}

fn convert_lit(lit: &syn::Lit) -> ConstValue {
    match lit {
        syn::Lit::Int(li) => ConstValue::Int(li.base10_parse::<i64>().unwrap_or(0)),
        syn::Lit::Float(lf) => ConstValue::Float(lf.base10_parse::<f64>().unwrap_or(0.0)),
        syn::Lit::Str(ls) => ConstValue::Str(ls.value()),
        syn::Lit::Bool(lb) => ConstValue::Bool(lb.value),
        syn::Lit::Char(lc) => ConstValue::Int(lc.value() as i64),
        _ => ConstValue::Null,
    }
}

/// Returns `Some(expr)` if the SymExpr is meaningful (not just Unknown).
fn meaningful(expr: SymExpr) -> Option<SymExpr> {
    if matches!(expr, SymExpr::Unknown) {
        None
    } else {
        Some(expr)
    }
}

// ─── Dependency Extraction ───────────────────────────────────────────────────

fn extract_dependencies(
    block: &syn::Block,
    param_names: &HashSet<String>,
) -> Vec<ExternalDependency> {
    let mut acc: HashMap<String, ExternalDependency> = HashMap::new();
    walk_block_for_deps(block, param_names, &mut acc);
    acc.into_values().collect()
}

fn walk_block_for_deps(
    block: &syn::Block,
    param_names: &HashSet<String>,
    acc: &mut HashMap<String, ExternalDependency>,
) {
    for stmt in &block.stmts {
        walk_stmt_for_deps(stmt, param_names, acc);
    }
}

fn walk_stmt_for_deps(
    stmt: &syn::Stmt,
    param_names: &HashSet<String>,
    acc: &mut HashMap<String, ExternalDependency>,
) {
    match stmt {
        syn::Stmt::Expr(expr, _) => {
            walk_expr_for_deps(expr, param_names, acc);
        }
        syn::Stmt::Local(local) => {
            if let Some(init) = &local.init {
                walk_expr_for_deps(&init.expr, param_names, acc);
                if let Some((_, diverge)) = &init.diverge {
                    walk_expr_for_deps(diverge, param_names, acc);
                }
            }
        }
        _ => {}
    }
}

fn walk_expr_for_deps(
    expr: &syn::Expr,
    param_names: &HashSet<String>,
    acc: &mut HashMap<String, ExternalDependency>,
) {
    match expr {
        syn::Expr::Call(call) => {
            // Check if the function is a qualified path (module::func)
            if let syn::Expr::Path(path) = call.func.as_ref()
                && path.path.segments.len() >= 2
            {
                let symbol = path
                    .path
                    .segments
                    .iter()
                    .map(|s| s.ident.to_string())
                    .collect::<Vec<_>>()
                    .join("::");
                let source_module = path
                    .path
                    .segments
                    .first()
                    .map(|s| s.ident.to_string())
                    .unwrap_or_default();
                let line = path
                    .path
                    .segments
                    .last()
                    .map(|s| Spanned::span(&s.ident).start().line as u32)
                    .unwrap_or(0);

                let entry = acc
                    .entry(symbol.clone())
                    .or_insert_with(|| ExternalDependency {
                        kind: DependencyKind::FunctionCall,
                        symbol,
                        source_module,
                        return_type: TypeInfo::Unknown,
                        param_types: vec![],
                        call_sites: vec![],
                    });
                entry.call_sites.push(line);
            }

            // Recurse into arguments
            for arg in &call.args {
                walk_expr_for_deps(arg, param_names, acc);
            }
        }

        syn::Expr::MethodCall(mc) => {
            // Check if receiver is NOT a parameter (external method call)
            let is_param_receiver = is_param_based_expr(&mc.receiver, param_names);
            if !is_param_receiver {
                let name = mc.method.to_string();
                let line = Spanned::span(&mc.method).start().line as u32;

                let entry = acc
                    .entry(name.clone())
                    .or_insert_with(|| ExternalDependency {
                        kind: DependencyKind::MethodCall,
                        symbol: name,
                        source_module: String::new(),
                        return_type: TypeInfo::Unknown,
                        param_types: vec![],
                        call_sites: vec![],
                    });
                entry.call_sites.push(line);
            }

            // Recurse
            walk_expr_for_deps(&mc.receiver, param_names, acc);
            for arg in &mc.args {
                walk_expr_for_deps(arg, param_names, acc);
            }
        }

        syn::Expr::If(e) => {
            walk_expr_for_deps(&e.cond, param_names, acc);
            walk_block_for_deps(&e.then_branch, param_names, acc);
            if let Some((_, else_branch)) = &e.else_branch {
                walk_expr_for_deps(else_branch, param_names, acc);
            }
        }

        syn::Expr::Match(e) => {
            walk_expr_for_deps(&e.expr, param_names, acc);
            for arm in &e.arms {
                walk_expr_for_deps(&arm.body, param_names, acc);
            }
        }

        syn::Expr::Block(e) => {
            walk_block_for_deps(&e.block, param_names, acc);
        }

        syn::Expr::While(e) => {
            walk_expr_for_deps(&e.cond, param_names, acc);
            walk_block_for_deps(&e.body, param_names, acc);
        }

        syn::Expr::ForLoop(e) => {
            walk_expr_for_deps(&e.expr, param_names, acc);
            walk_block_for_deps(&e.body, param_names, acc);
        }

        syn::Expr::Loop(e) => {
            walk_block_for_deps(&e.body, param_names, acc);
        }

        syn::Expr::Try(e) => {
            walk_expr_for_deps(&e.expr, param_names, acc);
        }

        syn::Expr::Return(e) => {
            if let Some(expr) = &e.expr {
                walk_expr_for_deps(expr, param_names, acc);
            }
        }

        syn::Expr::Binary(e) => {
            walk_expr_for_deps(&e.left, param_names, acc);
            walk_expr_for_deps(&e.right, param_names, acc);
        }

        syn::Expr::Unary(e) => {
            walk_expr_for_deps(&e.expr, param_names, acc);
        }

        syn::Expr::Paren(e) => {
            walk_expr_for_deps(&e.expr, param_names, acc);
        }

        syn::Expr::Reference(e) => {
            walk_expr_for_deps(&e.expr, param_names, acc);
        }

        syn::Expr::Closure(e) => {
            walk_expr_for_deps(&e.body, param_names, acc);
        }

        syn::Expr::Assign(e) => {
            walk_expr_for_deps(&e.right, param_names, acc);
        }

        syn::Expr::Unsafe(e) => {
            walk_block_for_deps(&e.block, param_names, acc);
        }

        syn::Expr::Let(e) => {
            walk_expr_for_deps(&e.expr, param_names, acc);
        }

        syn::Expr::Tuple(e) => {
            for elem in &e.elems {
                walk_expr_for_deps(elem, param_names, acc);
            }
        }

        _ => {}
    }
}

fn is_param_based_expr(expr: &syn::Expr, param_names: &HashSet<String>) -> bool {
    match expr {
        syn::Expr::Path(p) => p
            .path
            .get_ident()
            .is_some_and(|id| param_names.contains(&id.to_string())),
        syn::Expr::Field(f) => is_param_based_expr(&f.base, param_names),
        syn::Expr::Reference(r) => is_param_based_expr(&r.expr, param_names),
        syn::Expr::Paren(p) => is_param_based_expr(&p.expr, param_names),
        _ => false,
    }
}

// ─── Literal Extraction ──────────────────────────────────────────────────────

/// Walk a function block and collect all literal values, deduplicated.
fn extract_literals(block: &syn::Block) -> Vec<LiteralValue> {
    let mut seen = HashSet::new();
    let mut results = Vec::new();

    fn add(lit: LiteralValue, seen: &mut HashSet<String>, results: &mut Vec<LiteralValue>) {
        let key = serde_json::to_string(&lit).unwrap_or_default();
        if seen.insert(key) {
            results.push(lit);
        }
    }

    fn walk_expr(expr: &syn::Expr, seen: &mut HashSet<String>, results: &mut Vec<LiteralValue>) {
        match expr {
            syn::Expr::Lit(lit_expr) => match &lit_expr.lit {
                syn::Lit::Int(li) => {
                    if let Ok(v) = li.base10_parse::<i64>() {
                        add(LiteralValue::Int { value: v }, seen, results);
                    }
                }
                syn::Lit::Float(lf) => {
                    if let Ok(v) = lf.base10_parse::<f64>() {
                        add(LiteralValue::Float { value: v }, seen, results);
                    }
                }
                syn::Lit::Str(ls) => {
                    add(LiteralValue::Str { value: ls.value() }, seen, results);
                }
                syn::Lit::Bool(lb) => {
                    add(LiteralValue::Bool { value: lb.value }, seen, results);
                }
                syn::Lit::Byte(lb) => {
                    add(
                        LiteralValue::Int {
                            value: lb.value() as i64,
                        },
                        seen,
                        results,
                    );
                }
                syn::Lit::Char(lc) => {
                    add(
                        LiteralValue::Int {
                            value: lc.value() as i64,
                        },
                        seen,
                        results,
                    );
                }
                _ => {}
            },
            syn::Expr::Block(eb) => walk_block(&eb.block, seen, results),
            syn::Expr::If(ei) => {
                walk_expr(&ei.cond, seen, results);
                walk_block(&ei.then_branch, seen, results);
                if let Some((_, else_branch)) = &ei.else_branch {
                    walk_expr(else_branch, seen, results);
                }
            }
            syn::Expr::Match(em) => {
                walk_expr(&em.expr, seen, results);
                for arm in &em.arms {
                    walk_pat(&arm.pat, seen, results);
                    walk_expr(&arm.body, seen, results);
                }
            }
            syn::Expr::Binary(eb) => {
                walk_expr(&eb.left, seen, results);
                walk_expr(&eb.right, seen, results);
            }
            syn::Expr::Call(ec) => {
                walk_expr(&ec.func, seen, results);
                for arg in &ec.args {
                    walk_expr(arg, seen, results);
                }
                // Detect Regex::new("pattern")
                if let syn::Expr::Path(path_expr) = &*ec.func {
                    let seg_names: Vec<String> = path_expr
                        .path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect();
                    let is_regex_new = seg_names.last().map(|s| s == "new").unwrap_or(false)
                        && seg_names.iter().any(|s| s == "Regex");
                    if is_regex_new
                        && ec.args.len() == 1
                        && let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(ls),
                            ..
                        }) = &ec.args[0]
                    {
                        let key = format!("regex:{}", ls.value());
                        if seen.insert(key) {
                            results.push(LiteralValue::Regex {
                                pattern: ls.value(),
                            });
                        }
                    }
                }
            }
            syn::Expr::MethodCall(em) => {
                walk_expr(&em.receiver, seen, results);
                for arg in &em.args {
                    walk_expr(arg, seen, results);
                }
            }
            syn::Expr::Return(er) => {
                if let Some(expr) = &er.expr {
                    walk_expr(expr, seen, results);
                }
            }
            syn::Expr::Assign(ea) => {
                walk_expr(&ea.right, seen, results);
            }
            syn::Expr::Let(el) => {
                walk_expr(&el.expr, seen, results);
            }
            syn::Expr::Paren(ep) => walk_expr(&ep.expr, seen, results),
            syn::Expr::Unary(eu) => walk_expr(&eu.expr, seen, results),
            syn::Expr::Reference(er) => walk_expr(&er.expr, seen, results),
            syn::Expr::Tuple(et) => {
                for elem in &et.elems {
                    walk_expr(elem, seen, results);
                }
            }
            syn::Expr::Index(ei) => {
                walk_expr(&ei.expr, seen, results);
                walk_expr(&ei.index, seen, results);
            }
            syn::Expr::While(ew) => {
                walk_expr(&ew.cond, seen, results);
                walk_block(&ew.body, seen, results);
            }
            syn::Expr::ForLoop(ef) => {
                walk_expr(&ef.expr, seen, results);
                walk_block(&ef.body, seen, results);
            }
            syn::Expr::Loop(el) => walk_block(&el.body, seen, results),
            _ => {}
        }
    }

    fn walk_pat(pat: &syn::Pat, seen: &mut HashSet<String>, results: &mut Vec<LiteralValue>) {
        match pat {
            syn::Pat::Lit(pl) => {
                // PatLit holds a Lit, not an Expr; convert to ExprLit for walk_expr
                let expr_lit = syn::ExprLit {
                    attrs: vec![],
                    lit: pl.lit.clone(),
                };
                walk_expr(&syn::Expr::Lit(expr_lit), seen, results);
            }
            syn::Pat::Range(pr) => {
                if let Some(start) = &pr.start {
                    walk_expr(start, seen, results);
                }
                if let Some(end) = &pr.end {
                    walk_expr(end, seen, results);
                }
            }
            syn::Pat::Or(po) => {
                for case in &po.cases {
                    walk_pat(case, seen, results);
                }
            }
            syn::Pat::Tuple(pt) => {
                for elem in &pt.elems {
                    walk_pat(elem, seen, results);
                }
            }
            _ => {}
        }
    }

    fn walk_stmt(stmt: &syn::Stmt, seen: &mut HashSet<String>, results: &mut Vec<LiteralValue>) {
        match stmt {
            syn::Stmt::Expr(expr, _) => walk_expr(expr, seen, results),
            syn::Stmt::Local(local) => {
                if let Some(init) = &local.init {
                    walk_expr(&init.expr, seen, results);
                }
            }
            syn::Stmt::Item(_) | syn::Stmt::Macro(_) => {}
        }
    }

    fn walk_block(block: &syn::Block, seen: &mut HashSet<String>, results: &mut Vec<LiteralValue>) {
        for stmt in &block.stmts {
            walk_stmt(stmt, seen, results);
        }
    }

    walk_block(block, &mut seen, &mut results);
    results
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn analyze(code: &str) -> Vec<FunctionAnalysis> {
        analyze_source(code, None).expect("analysis should succeed")
    }

    fn analyze_fn(code: &str, name: &str) -> FunctionAnalysis {
        analyze_source(code, Some(name))
            .expect("analysis should succeed")
            .into_iter()
            .next()
            .expect("function should be found")
    }

    /// Shorthand for a sized `TypeInfo::Int` in test assertions.
    fn int_ty(width: u8, signed: bool) -> TypeInfo {
        TypeInfo::Int {
            int_width: Some(width),
            int_signed: Some(signed),
        }
    }

    // ── Async detection tests ──

    #[test]
    fn sync_fn_not_async() {
        let f = analyze_fn("fn f() {}", "f");
        assert!(!f.is_async);
    }

    #[test]
    fn async_fn_detected() {
        let f = analyze_fn("async fn f() {}", "f");
        assert!(f.is_async);
    }

    // ── Type mapping tests ──

    #[test]
    fn maps_i32_to_int() {
        let f = analyze_fn("fn f(x: i32) {}", "f");
        assert_eq!(f.params[0].typ, int_ty(32, true));
        assert_eq!(f.params[0].name, "x");
    }

    #[test]
    fn maps_u64_to_int() {
        let f = analyze_fn("fn f(x: u64) {}", "f");
        assert_eq!(f.params[0].typ, int_ty(64, false));
    }

    #[test]
    fn maps_u8_to_sized_unsigned_int() {
        let f = analyze_fn("fn f(x: u8) {}", "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Int {
                int_width: Some(8),
                int_signed: Some(false),
            }
        );
    }

    #[test]
    fn maps_i8_isize_usize_widths() {
        assert_eq!(
            analyze_fn("fn f(x: i8) {}", "f").params[0].typ,
            int_ty(8, true)
        );
        assert_eq!(
            analyze_fn("fn f(x: usize) {}", "f").params[0].typ,
            int_ty(64, false)
        );
        assert_eq!(
            analyze_fn("fn f(x: isize) {}", "f").params[0].typ,
            int_ty(64, true)
        );
        assert_eq!(
            analyze_fn("fn f(x: u128) {}", "f").params[0].typ,
            int_ty(128, false)
        );
    }

    #[test]
    fn maps_f64_to_float() {
        let f = analyze_fn("fn f(x: f64) {}", "f");
        assert_eq!(f.params[0].typ, TypeInfo::Float);
    }

    #[test]
    fn maps_string_to_str() {
        let f = analyze_fn("fn f(x: String) {}", "f");
        assert_eq!(f.params[0].typ, TypeInfo::Str);
    }

    #[test]
    fn maps_str_ref_to_str() {
        let f = analyze_fn("fn f(x: &str) {}", "f");
        assert_eq!(f.params[0].typ, TypeInfo::Str);
    }

    #[test]
    fn maps_bool_to_bool() {
        let f = analyze_fn("fn f(x: bool) {}", "f");
        assert_eq!(f.params[0].typ, TypeInfo::Bool);
    }

    #[test]
    fn maps_vec_to_array() {
        let f = analyze_fn("fn f(x: Vec<i32>) {}", "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Array {
                element: Box::new(int_ty(32, true))
            }
        );
    }

    #[test]
    fn maps_option_to_nullable() {
        let f = analyze_fn("fn f(x: Option<String>) {}", "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Nullable {
                inner: Box::new(TypeInfo::Str)
            }
        );
    }

    #[test]
    fn maps_result_to_union() {
        let f = analyze_fn("fn f(x: Result<i32, String>) {}", "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Union {
                variants: vec![int_ty(32, true), TypeInfo::Str],
                enum_values: Vec::new(),
            }
        );
    }

    #[test]
    fn unwraps_box() {
        let f = analyze_fn("fn f(x: Box<i32>) {}", "f");
        assert_eq!(f.params[0].typ, int_ty(32, true));
    }

    #[test]
    fn unwraps_reference() {
        let f = analyze_fn("fn f(x: &i32) {}", "f");
        assert_eq!(f.params[0].typ, int_ty(32, true));
    }

    #[test]
    fn maps_tuple_to_object() {
        let f = analyze_fn("fn f(x: (i32, String)) {}", "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Object {
                fields: vec![
                    ("0".to_string(), int_ty(32, true)),
                    ("1".to_string(), TypeInfo::Str),
                ]
            }
        );
    }

    #[test]
    fn maps_slice_to_array() {
        let f = analyze_fn("fn f(x: &[u8]) {}", "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Array {
                element: Box::new(int_ty(8, false))
            }
        );
    }

    #[test]
    fn maps_array_literal_to_array() {
        let f = analyze_fn("fn f(x: [i32; 5]) {}", "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Array {
                element: Box::new(int_ty(32, true))
            }
        );
    }

    #[test]
    fn maps_pathbuf_to_complex_path() {
        let f = analyze_fn("fn f(x: PathBuf) {}", "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Complex {
                kind: ComplexKind::Path,
                metadata: HashMap::new(),
                inner: None,
            }
        );
    }

    #[test]
    fn serde_json_value_synthesizes_as_unknown() {
        // serde_json::Value (bare or qualified) is dynamic JSON — synthesizable,
        // never opaque (str-orku).
        assert_eq!(
            analyze_fn("fn f(x: Value) {}", "f").params[0].typ,
            TypeInfo::Unknown
        );
        assert_eq!(
            analyze_fn("fn f(x: serde_json::Value) {}", "f").params[0].typ,
            TypeInfo::Unknown
        );
    }

    #[test]
    fn serde_json_map_synthesizes_as_object() {
        let f = analyze_fn("fn f(x: Map<String, Value>) {}", "f");
        assert!(
            matches!(f.params[0].typ, TypeInfo::Object { .. }),
            "expected Object for serde_json::Map, got {:?}",
            f.params[0].typ
        );
    }

    #[test]
    fn apply_rename_all_conventions() {
        assert_eq!(apply_rename_all("workspace_id", "camelCase"), "workspaceId");
        assert_eq!(
            apply_rename_all("workspace_id", "PascalCase"),
            "WorkspaceId"
        );
        assert_eq!(
            apply_rename_all("workspace_id", "snake_case"),
            "workspace_id"
        );
        assert_eq!(
            apply_rename_all("workspace_id", "SCREAMING_SNAKE_CASE"),
            "WORKSPACE_ID"
        );
        assert_eq!(
            apply_rename_all("workspace_id", "kebab-case"),
            "workspace-id"
        );
        assert_eq!(apply_rename_all("workspace_id", "lowercase"), "workspaceid");
        assert_eq!(apply_rename_all("id", "camelCase"), "id");
    }

    #[test]
    fn struct_fields_honor_serde_rename_all() {
        // str-55ep: a struct declaring rename_all="camelCase" must synthesize
        // camelCase JSON keys, or crate_bridge deserialization fails.
        let code = "#[serde(rename_all = \"camelCase\")] struct Item { workspace_id: u32, item_category_id: u32, name: String } fn f(x: Item) {}";
        let f = analyze_fn(code, "f");
        match &f.params[0].typ {
            TypeInfo::Object { fields } => {
                let keys: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
                assert!(keys.contains(&"workspaceId"), "got {keys:?}");
                assert!(keys.contains(&"itemCategoryId"), "got {keys:?}");
                assert!(keys.contains(&"name"), "got {keys:?}");
            }
            other => panic!("expected Object, got {other:?}"),
        }
    }

    #[test]
    fn field_serde_rename_overrides_rename_all() {
        let code = "#[serde(rename_all = \"camelCase\")] struct S { #[serde(rename = \"customKey\")] my_field: u32 } fn f(x: S) {}";
        let f = analyze_fn(code, "f");
        match &f.params[0].typ {
            TypeInfo::Object { fields } => {
                assert!(
                    fields.iter().any(|(k, _)| k == "customKey"),
                    "expected customKey, got {:?}",
                    fields.iter().map(|(k, _)| k).collect::<Vec<_>>()
                );
            }
            other => panic!("expected Object, got {other:?}"),
        }
    }

    #[test]
    fn maps_unknown_struct_to_opaque() {
        let f = analyze_fn("fn f(x: MyStruct) {}", "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Opaque {
                label: "MyStruct".to_string()
            }
        );
    }

    // ── Cross-file (same-crate) struct/enum resolution (str-do53) ──

    /// Build a throwaway crate on disk: `Cargo.toml` + `src/<file>.rs` for each
    /// (name, source) pair. Returns the crate root tempdir (keep it alive) and
    /// the path to the first file.
    fn make_crate(files: &[(&str, &str)]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().expect("create tempdir");
        let root = dir.path();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"xcrate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("write Cargo.toml");
        let src = root.join("src");
        std::fs::create_dir(&src).expect("create src");
        let mut first = None;
        for (fname, source) in files {
            let path = src.join(fname);
            std::fs::write(&path, source).expect("write source file");
            if first.is_none() {
                first = Some(path);
            }
        }
        let first = first.expect("at least one file");
        (dir, first)
    }

    #[test]
    fn cross_file_struct_resolves_to_object() {
        let (_dir, logic) = make_crate(&[
            (
                "logic.rs",
                "use crate::domain::Widget;\npub fn describe(w: &Widget) -> usize { w.tags.len() }",
            ),
            (
                "domain.rs",
                "pub struct Widget { pub id: u32, pub name: String, pub tags: Vec<String> }",
            ),
        ]);
        let fns = analyze_file(&logic, Some("describe")).expect("analysis should succeed");
        let f = &fns[0];
        match &f.params[0].typ {
            TypeInfo::Object { fields } => {
                let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
                assert!(
                    names.contains(&"id") && names.contains(&"name") && names.contains(&"tags"),
                    "expected id/name/tags fields, got {names:?}"
                );
            }
            other => panic!("expected Object from cross-file struct, got {other:?}"),
        }
    }

    #[test]
    fn cross_file_ambiguous_bare_name_stays_opaque() {
        // Two files define `Widget`; the bare name is ambiguous and must not be
        // synthesized cross-file (could be the wrong type). It stays Opaque.
        let (_dir, logic) = make_crate(&[
            (
                "logic.rs",
                "use crate::a::Widget;\npub fn describe(w: &Widget) -> u32 { w.id }",
            ),
            ("a.rs", "pub struct Widget { pub id: u32 }"),
            ("b.rs", "pub struct Widget { pub other: String }"),
        ]);
        let fns = analyze_file(&logic, Some("describe")).expect("analysis should succeed");
        assert_eq!(
            fns[0].params[0].typ,
            TypeInfo::Opaque {
                label: "Widget".to_string()
            },
            "ambiguous cross-file bare name must stay Opaque"
        );
    }

    #[test]
    fn cross_file_struct_enum_name_collision_stays_opaque() {
        // `Widget` is a struct in one file and an enum in another. The bare name
        // is ambiguous across kinds and must not silently resolve to either.
        let (_dir, logic) = make_crate(&[
            (
                "logic.rs",
                "use crate::a::Widget;\npub fn describe(w: &Widget) -> u32 { 0 }",
            ),
            ("a.rs", "pub struct Widget { pub id: u32 }"),
            ("b.rs", "pub enum Widget { On, Off }"),
        ]);
        let fns = analyze_file(&logic, Some("describe")).expect("analysis should succeed");
        assert_eq!(
            fns[0].params[0].typ,
            TypeInfo::Opaque {
                label: "Widget".to_string()
            },
            "struct/enum cross-kind name collision must stay Opaque"
        );
    }

    #[test]
    fn same_file_struct_wins_over_cross_file() {
        // `logic.rs` defines its own `Widget`; a different `Widget` in another
        // file must not shadow it. The local definition (field `local`) wins.
        let (_dir, logic) = make_crate(&[
            (
                "logic.rs",
                "pub struct Widget { pub local: bool }\npub fn describe(w: &Widget) -> bool { w.local }",
            ),
            ("domain.rs", "pub struct Widget { pub remote: u32 }"),
        ]);
        let fns = analyze_file(&logic, Some("describe")).expect("analysis should succeed");
        match &fns[0].params[0].typ {
            TypeInfo::Object { fields } => {
                let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
                assert_eq!(names, vec!["local"], "same-file struct must win");
            }
            other => panic!("expected Object, got {other:?}"),
        }
    }

    #[test]
    fn cross_file_self_referential_struct_terminates() {
        // A self-referential cross-file struct must not infinitely recurse.
        let (_dir, logic) = make_crate(&[
            (
                "logic.rs",
                "use crate::domain::Node;\npub fn depth(n: &Node) -> u32 { 0 }",
            ),
            (
                "domain.rs",
                "pub struct Node { pub value: i32, pub next: Option<Box<Node>> }",
            ),
        ]);
        let fns = analyze_file(&logic, Some("depth")).expect("analysis should succeed");
        // Must resolve to an Object (top level) without hanging.
        assert!(
            matches!(fns[0].params[0].typ, TypeInfo::Object { .. }),
            "expected Object for self-referential struct, got {:?}",
            fns[0].params[0].typ
        );
    }

    #[test]
    fn maps_same_file_struct_to_object() {
        let code = r#"
            struct Order {
                id: i32,
                name: String,
            }
            fn f(o: Order) {}
        "#;
        let f = analyze_fn(code, "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Object {
                fields: vec![
                    ("id".to_string(), int_ty(32, true)),
                    ("name".to_string(), TypeInfo::Str),
                ]
            }
        );
        assert_eq!(f.params[0].type_name.as_deref(), Some("Order"));
    }

    // ── Return type tests ──

    #[test]
    fn extracts_return_type() {
        let f = analyze_fn("fn f() -> i32 { 0 }", "f");
        assert_eq!(f.return_type, int_ty(32, true));
    }

    #[test]
    fn default_return_type_is_unknown() {
        let f = analyze_fn("fn f() {}", "f");
        assert_eq!(f.return_type, TypeInfo::Unknown);
    }

    // ── Visibility tests ──

    #[test]
    fn detects_public_functions() {
        let fns = analyze("pub fn exported() {} fn private() {}");
        assert_eq!(fns.len(), 2);
        assert!(fns[0].exported);
        assert!(!fns[1].exported);
    }

    // ── Branch extraction tests ──

    #[test]
    fn extracts_if_branch() {
        let f = analyze_fn(
            r#"
            fn f(x: i32) {
                if x > 5 {
                    println!("big");
                }
            }
            "#,
            "f",
        );
        assert_eq!(f.branches.len(), 1);
        assert_eq!(f.branches[0].branch_type, BranchType::If);
        assert!(f.branches[0].condition_text.contains("x > 5"));
    }

    #[test]
    fn extracts_if_else_if_branches() {
        let f = analyze_fn(
            r#"
            fn f(x: i32) {
                if x > 10 {
                    println!("big");
                } else if x > 0 {
                    println!("small");
                } else {
                    println!("neg");
                }
            }
            "#,
            "f",
        );
        assert!(f.branches.len() >= 2);
        assert_eq!(f.branches[0].branch_type, BranchType::If);
        assert_eq!(f.branches[1].branch_type, BranchType::ElseIf);
    }

    #[test]
    fn extracts_match_arms() {
        let f = analyze_fn(
            r#"
            fn f(x: i32) -> &'static str {
                match x {
                    0 => "zero",
                    1 => "one",
                    _ => "other",
                }
            }
            "#,
            "f",
        );
        let switch_branches: Vec<_> = f
            .branches
            .iter()
            .filter(|b| b.branch_type == BranchType::Switch)
            .collect();
        assert_eq!(switch_branches.len(), 3);
    }

    #[test]
    fn extracts_while_branch() {
        let f = analyze_fn(
            r#"
            fn f(x: i32) {
                let mut i = 0;
                while i < x {
                    i += 1;
                }
            }
            "#,
            "f",
        );
        let while_branches: Vec<_> = f
            .branches
            .iter()
            .filter(|b| b.branch_type == BranchType::While)
            .collect();
        assert_eq!(while_branches.len(), 1);
    }

    #[test]
    fn extracts_for_branch() {
        let f = analyze_fn(
            r#"
            fn f(items: Vec<i32>) {
                for item in &items {
                    println!("{}", item);
                }
            }
            "#,
            "f",
        );
        let for_branches: Vec<_> = f
            .branches
            .iter()
            .filter(|b| b.branch_type == BranchType::For)
            .collect();
        assert_eq!(for_branches.len(), 1);
    }

    #[test]
    fn extracts_try_operator_branch() {
        let f = analyze_fn(
            r#"
            fn f(input: &str) -> Result<i32, String> {
                let n = input.parse::<i32>().map_err(|e| e.to_string())?;
                Ok(n)
            }
            "#,
            "f",
        );
        // The ? operator should produce a branch
        let try_branches: Vec<_> = f
            .branches
            .iter()
            .filter(|b| b.condition_text.contains('?'))
            .collect();
        assert!(!try_branches.is_empty());
    }

    #[test]
    fn extracts_loop_branch() {
        let f = analyze_fn(
            r#"
            fn f() {
                loop {
                    break;
                }
            }
            "#,
            "f",
        );
        let loop_branches: Vec<_> = f
            .branches
            .iter()
            .filter(|b| b.condition_text == "loop")
            .collect();
        assert_eq!(loop_branches.len(), 1);
    }

    // ── SymExpr tests ──

    #[test]
    fn builds_param_sym_expr() {
        let params: HashSet<String> = ["x".to_string()].into();
        let expr: syn::Expr = syn::parse_str("x").expect("parse");
        let sym = build_sym_expr(&expr, &params);
        assert_eq!(
            sym,
            SymExpr::Param {
                name: "x".to_string(),
                path: vec![],
            }
        );
    }

    #[test]
    fn builds_field_access_sym_expr() {
        let params: HashSet<String> = ["order".to_string()].into();
        let expr: syn::Expr = syn::parse_str("order.priority").expect("parse");
        let sym = build_sym_expr(&expr, &params);
        assert_eq!(
            sym,
            SymExpr::Param {
                name: "order".to_string(),
                path: vec!["priority".to_string()],
            }
        );
    }

    #[test]
    fn builds_nested_field_access_sym_expr_root_first() {
        // str-do53 review: nested chains must stay root-first; a leaf-first
        // path (["priority", "meta"]) would solve/overlay the wrong shape.
        let params: HashSet<String> = ["order".to_string()].into();
        let expr: syn::Expr = syn::parse_str("order.meta.priority").expect("parse");
        let sym = build_sym_expr(&expr, &params);
        assert_eq!(
            sym,
            SymExpr::Param {
                name: "order".to_string(),
                path: vec!["meta".to_string(), "priority".to_string()],
            }
        );
    }

    #[test]
    fn builds_binary_op_sym_expr() {
        let params: HashSet<String> = ["x".to_string()].into();
        let expr: syn::Expr = syn::parse_str("x > 5").expect("parse");
        let sym = build_sym_expr(&expr, &params);
        assert_eq!(
            sym,
            SymExpr::BinOp {
                op: BinOpKind::Gt,
                left: Box::new(SymExpr::Param {
                    name: "x".to_string(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(5))),
            }
        );
    }

    #[test]
    fn builds_unary_not_sym_expr() {
        let params: HashSet<String> = ["valid".to_string()].into();
        let expr: syn::Expr = syn::parse_str("!valid").expect("parse");
        let sym = build_sym_expr(&expr, &params);
        assert_eq!(
            sym,
            SymExpr::UnOp {
                op: UnOpKind::Not,
                operand: Box::new(SymExpr::Param {
                    name: "valid".to_string(),
                    path: vec![],
                }),
            }
        );
    }

    #[test]
    fn builds_method_call_sym_expr() {
        let params: HashSet<String> = ["s".to_string()].into();
        let expr: syn::Expr = syn::parse_str(r#"s.starts_with("/api")"#).expect("parse");
        let sym = build_sym_expr(&expr, &params);
        assert_eq!(
            sym,
            SymExpr::Call {
                name: "starts_with".to_string(),
                receiver: Some(Box::new(SymExpr::Param {
                    name: "s".to_string(),
                    path: vec![],
                })),
                args: vec![SymExpr::Const(ConstValue::Str("/api".to_string()))],
            }
        );
    }

    #[test]
    fn non_param_is_unknown() {
        let params: HashSet<String> = ["x".to_string()].into();
        let expr: syn::Expr = syn::parse_str("local_var").expect("parse");
        let sym = build_sym_expr(&expr, &params);
        assert_eq!(sym, SymExpr::Unknown);
    }

    // ── Dependency tests ──

    #[test]
    fn detects_qualified_function_call_dep() {
        let f = analyze_fn(
            r#"
            fn f(x: i32) {
                db::save(x);
            }
            "#,
            "f",
        );
        assert_eq!(f.dependencies.len(), 1);
        assert_eq!(f.dependencies[0].symbol, "db::save");
        assert_eq!(f.dependencies[0].source_module, "db");
        assert_eq!(f.dependencies[0].kind, DependencyKind::FunctionCall);
    }

    #[test]
    fn groups_multiple_calls_to_same_symbol() {
        let f = analyze_fn(
            r#"
            fn f(x: i32) {
                db::save(x);
                db::save(x + 1);
            }
            "#,
            "f",
        );
        assert_eq!(f.dependencies.len(), 1);
        assert_eq!(f.dependencies[0].call_sites.len(), 2);
    }

    // ── Error handling tests ──

    #[test]
    fn function_not_found_returns_error() {
        let result = analyze_source("fn f() {}", Some("nonexistent"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AnalyzeError::FunctionNotFound(_)));
    }

    #[test]
    fn parse_error_returns_error() {
        let result = analyze_source("fn f( {}", None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, AnalyzeError::ParseError(_)));
    }

    // ── Filter by function name ──

    #[test]
    fn filters_by_function_name() {
        let code = "fn a() {} fn b() {} fn c() {}";
        let result = analyze_source(code, Some("b")).expect("success");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "b");
    }

    #[test]
    fn returns_all_functions_when_no_filter() {
        let code = "fn a() {} fn b() {} fn c() {}";
        let result = analyze_source(code, None).expect("success");
        assert_eq!(result.len(), 3);
    }

    // ── Integration: full analysis ──

    #[test]
    fn full_analysis_of_classify_function() {
        let code = r#"
            pub fn classify(n: i32) -> &'static str {
                if n < 0 {
                    "negative"
                } else if n == 0 {
                    "zero"
                } else {
                    "positive"
                }
            }
        "#;
        let f = analyze_fn(code, "classify");
        assert_eq!(f.name, "classify");
        assert!(f.exported);
        assert_eq!(f.params.len(), 1);
        assert_eq!(f.params[0].name, "n");
        assert_eq!(f.params[0].typ, int_ty(32, true));
        assert_eq!(f.return_type, TypeInfo::Str);
        assert!(f.branches.len() >= 2); // if + else-if at least
        assert_eq!(f.branches[0].branch_type, BranchType::If);

        // Verify symbolic condition for first branch
        assert_eq!(
            f.branches[0].condition,
            Some(SymExpr::BinOp {
                op: BinOpKind::Lt,
                left: Box::new(SymExpr::Param {
                    name: "n".to_string(),
                    path: vec![],
                }),
                right: Box::new(SymExpr::Const(ConstValue::Int(0))),
            })
        );
    }

    // ── Literal extraction tests ──

    #[test]
    fn extract_literals_finds_strings_in_if_condition() {
        let f = analyze_fn(
            r#"pub fn classify(s: &str) -> &str {
                if s == "express" { return "fast"; }
                "slow"
            }"#,
            "classify",
        );
        let strs: Vec<&str> = f
            .literals
            .iter()
            .filter_map(|l| {
                if let LiteralValue::Str { value } = l {
                    Some(value.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(strs.contains(&"express"), "should find 'express'");
        assert!(strs.contains(&"fast"), "should find 'fast'");
        assert!(strs.contains(&"slow"), "should find 'slow'");
    }

    #[test]
    fn extract_literals_finds_ints_in_match_arm() {
        let f = analyze_fn(
            r#"pub fn grade(n: i32) -> &str {
                match n {
                    90 => "A",
                    70 => "B",
                    _ => "F",
                }
            }"#,
            "grade",
        );
        let ints: Vec<i64> = f
            .literals
            .iter()
            .filter_map(|l| {
                if let LiteralValue::Int { value } = l {
                    Some(*value)
                } else {
                    None
                }
            })
            .collect();
        assert!(ints.contains(&90));
        assert!(ints.contains(&70));
    }

    #[test]
    fn extract_literals_deduplicates_repeated_values() {
        let f = analyze_fn(
            r#"pub fn f(s: &str) -> bool {
                s == "ok" || s == "ok" || s == "ok"
            }"#,
            "f",
        );
        let ok_count = f
            .literals
            .iter()
            .filter(|l| matches!(l, LiteralValue::Str { value } if value == "ok"))
            .count();
        assert_eq!(ok_count, 1);
    }

    #[test]
    fn extract_literals_empty_for_no_literals() {
        let f = analyze_fn(r#"pub fn identity(x: i32) -> i32 { x }"#, "identity");
        assert!(f.literals.is_empty());
    }

    #[test]
    fn extract_literals_finds_bool() {
        let f = analyze_fn(r#"pub fn check() -> bool { true }"#, "check");
        assert!(
            f.literals
                .iter()
                .any(|l| matches!(l, LiteralValue::Bool { value: true }))
        );
    }

    // ── Enum type mapping tests ──

    #[test]
    fn maps_unit_enum_to_union_of_unknowns() {
        let code = r#"
            enum Direction { North, South, East, West }
            fn f(d: Direction) {}
        "#;
        let f = analyze_fn(code, "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Union {
                variants: vec![
                    TypeInfo::Unknown,
                    TypeInfo::Unknown,
                    TypeInfo::Unknown,
                    TypeInfo::Unknown,
                ],
                enum_values: Vec::new(),
            }
        );
    }

    #[test]
    fn maps_data_enum_to_union_with_payload_types() {
        let code = r#"
            enum Shape {
                Circle(f64),
                Rect { w: f64, h: f64 },
                Point,
            }
            fn f(s: Shape) {}
        "#;
        let f = analyze_fn(code, "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Union {
                variants: vec![
                    TypeInfo::Float,
                    TypeInfo::Object {
                        fields: vec![
                            ("w".to_string(), TypeInfo::Float),
                            ("h".to_string(), TypeInfo::Float),
                        ]
                    },
                    TypeInfo::Unknown,
                ],
                enum_values: Vec::new(),
            }
        );
    }

    #[test]
    fn maps_multi_field_tuple_variant() {
        let code = r#"
            enum Pair { Two(i32, String) }
            fn f(p: Pair) {}
        "#;
        let f = analyze_fn(code, "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Union {
                variants: vec![TypeInfo::Object {
                    fields: vec![
                        ("0".to_string(), int_ty(32, true)),
                        ("1".to_string(), TypeInfo::Str),
                    ]
                }],
                enum_values: Vec::new(),
            }
        );
    }

    #[test]
    fn recursive_enum_does_not_infinite_loop() {
        let code = r#"
            enum List { Cons(i32, Box<List>), Nil }
            fn f(l: List) {}
        "#;
        let f = analyze_fn(code, "f");
        assert!(matches!(f.params[0].typ, TypeInfo::Union { .. }));
    }

    #[test]
    fn enum_not_in_same_file_remains_opaque() {
        let f = analyze_fn("fn f(x: ExternalEnum) {}", "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Opaque {
                label: "ExternalEnum".to_string()
            }
        );
    }

    // ── Generic type parameter tests ──

    #[test]
    fn generic_type_param_maps_to_unknown() {
        let f = analyze_fn("fn f<T>(x: T) {}", "f");
        assert_eq!(f.params[0].typ, TypeInfo::Unknown);
    }

    #[test]
    fn generic_param_with_bounds_maps_to_unknown() {
        let f = analyze_fn("fn f<T: std::fmt::Display + Clone>(x: T) {}", "f");
        assert_eq!(f.params[0].typ, TypeInfo::Unknown);
    }

    #[test]
    fn generic_return_type_maps_to_unknown() {
        let f = analyze_fn("fn f<T>() -> T { todo!() }", "f");
        assert_eq!(f.return_type, TypeInfo::Unknown);
    }

    #[test]
    fn generic_in_container_maps_inner_to_unknown() {
        let f = analyze_fn("fn f<T>(x: Vec<T>) {}", "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Array {
                element: Box::new(TypeInfo::Unknown)
            }
        );
    }

    #[test]
    fn multiple_generic_params() {
        let f = analyze_fn("fn f<K, V>(k: K, v: V) {}", "f");
        assert_eq!(f.params[0].typ, TypeInfo::Unknown);
        assert_eq!(f.params[1].typ, TypeInfo::Unknown);
    }

    #[test]
    fn non_generic_named_type_still_opaque() {
        let f = analyze_fn("fn f(x: MyStruct) {}", "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Opaque {
                label: "MyStruct".to_string()
            }
        );
    }

    // ── Pattern matching tests ──

    #[test]
    fn match_pat_lit_produces_eq_condition() {
        let f = analyze_fn(
            r#"fn f(x: i32) -> &'static str {
                match x {
                    42 => "answer",
                    _ => "other",
                }
            }"#,
            "f",
        );
        let switch: Vec<_> = f
            .branches
            .iter()
            .filter(|b| b.branch_type == BranchType::Switch)
            .collect();
        assert!(
            switch[0].condition.is_some(),
            "Pat::Lit arm should have a condition"
        );
        assert!(matches!(
            &switch[0].condition,
            Some(SymExpr::BinOp {
                op: BinOpKind::Eq,
                right,
                ..
            }) if matches!(right.as_ref(), SymExpr::Const(ConstValue::Int(42)))
        ));
    }

    #[test]
    fn match_pat_range_produces_and_of_bounds() {
        let f = analyze_fn(
            r#"fn f(x: i32) -> &'static str {
                match x {
                    1..=10 => "low",
                    _ => "high",
                }
            }"#,
            "f",
        );
        let switch: Vec<_> = f
            .branches
            .iter()
            .filter(|b| b.branch_type == BranchType::Switch)
            .collect();
        assert!(
            switch[0].condition.is_some(),
            "Pat::Range arm should have a condition"
        );
        assert!(matches!(
            &switch[0].condition,
            Some(SymExpr::BinOp {
                op: BinOpKind::And,
                ..
            })
        ));
    }

    #[test]
    fn match_pat_or_produces_or_chain() {
        let f = analyze_fn(
            r#"fn f(x: i32) -> &'static str {
                match x {
                    1 | 2 | 3 => "small",
                    _ => "big",
                }
            }"#,
            "f",
        );
        let switch: Vec<_> = f
            .branches
            .iter()
            .filter(|b| b.branch_type == BranchType::Switch)
            .collect();
        assert!(
            switch[0].condition.is_some(),
            "Pat::Or arm should have a condition"
        );
        assert!(matches!(
            &switch[0].condition,
            Some(SymExpr::BinOp {
                op: BinOpKind::Or,
                ..
            })
        ));
    }

    #[test]
    fn match_tuple_struct_pattern_produces_variant_eq() {
        let f = analyze_fn(
            r#"fn f(opt: Option<i32>) -> i32 {
                match opt {
                    Some(v) => v,
                    None => 0,
                }
            }"#,
            "f",
        );
        let switch: Vec<_> = f
            .branches
            .iter()
            .filter(|b| b.branch_type == BranchType::Switch)
            .collect();
        assert!(switch[0].condition.is_some());
        assert!(matches!(
            &switch[0].condition,
            Some(SymExpr::BinOp {
                op: BinOpKind::Eq,
                right,
                ..
            }) if matches!(right.as_ref(), SymExpr::Const(ConstValue::Str(s)) if s == "Some")
        ));
    }

    #[test]
    fn match_struct_pattern_produces_condition() {
        let code = r#"
            struct Point { x: i32, y: i32 }
            fn f(p: Point) -> i32 {
                match p {
                    Point { x, .. } => x,
                }
            }
        "#;
        let f = analyze_fn(code, "f");
        let switch: Vec<_> = f
            .branches
            .iter()
            .filter(|b| b.branch_type == BranchType::Switch)
            .collect();
        assert!(switch[0].condition.is_some());
    }

    #[test]
    fn match_wildcard_no_condition() {
        let f = analyze_fn(r#"fn f(x: i32) -> i32 { match x { _ => 0 } }"#, "f");
        let switch: Vec<_> = f
            .branches
            .iter()
            .filter(|b| b.branch_type == BranchType::Switch)
            .collect();
        assert!(switch[0].condition.is_none());
    }

    #[test]
    fn match_path_pattern_produces_variant_eq() {
        let code = r#"
            enum Status { Active, Inactive }
            fn f(s: Status) -> i32 {
                match s {
                    Status::Active => 1,
                    Status::Inactive => 0,
                }
            }
        "#;
        let f = analyze_fn(code, "f");
        let switch: Vec<_> = f
            .branches
            .iter()
            .filter(|b| b.branch_type == BranchType::Switch)
            .collect();
        assert!(switch[0].condition.is_some());
        assert!(matches!(
            &switch[0].condition,
            Some(SymExpr::BinOp {
                op: BinOpKind::Eq,
                right,
                ..
            }) if matches!(right.as_ref(), SymExpr::Const(ConstValue::Str(s)) if s == "Active")
        ));
    }

    // ── if-let pattern tests ──

    #[test]
    fn if_let_some_produces_variant_condition() {
        let f = analyze_fn(
            r#"fn f(opt: Option<i32>) -> i32 {
                if let Some(v) = opt { v } else { 0 }
            }"#,
            "f",
        );
        let branch = &f.branches[0];
        assert_eq!(branch.branch_type, BranchType::If);
        assert!(
            branch.condition.is_some(),
            "if-let should produce a condition"
        );
        assert!(matches!(
            &branch.condition,
            Some(SymExpr::BinOp {
                op: BinOpKind::Eq,
                right,
                ..
            }) if matches!(right.as_ref(), SymExpr::Const(ConstValue::Str(s)) if s == "Some")
        ));
    }

    #[test]
    fn if_let_ok_produces_variant_condition() {
        let f = analyze_fn(
            r#"fn f(r: Result<i32, String>) -> i32 {
                if let Ok(v) = r { v } else { -1 }
            }"#,
            "f",
        );
        let branch = &f.branches[0];
        assert!(branch.condition.is_some());
        assert!(matches!(
            &branch.condition,
            Some(SymExpr::BinOp {
                op: BinOpKind::Eq,
                right,
                ..
            }) if matches!(right.as_ref(), SymExpr::Const(ConstValue::Str(s)) if s == "Ok")
        ));
    }

    // ── Trait object type tests ──

    #[test]
    fn dyn_trait_ref_maps_to_opaque() {
        let code = r#"
            trait DataStore { fn get(&self) -> i32; }
            fn f(store: &dyn DataStore) -> i32 { store.get() }
        "#;
        let f = analyze_fn(code, "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Opaque {
                label: "dyn DataStore".to_string()
            }
        );
    }

    #[test]
    fn dyn_trait_mut_ref_maps_to_opaque() {
        let f = analyze_fn(
            "trait W { fn write(&mut self); } fn f(w: &mut dyn W) {}",
            "f",
        );
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Opaque {
                label: "dyn W".to_string()
            }
        );
    }

    #[test]
    fn dyn_trait_with_multiple_bounds_maps_to_opaque() {
        let f = analyze_fn("fn f(x: &(dyn std::fmt::Debug + Send)) {}", "f");
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Opaque {
                label: "dyn std::fmt::Debug + Send".to_string()
            }
        );
    }

    #[test]
    fn box_dyn_trait_maps_to_opaque() {
        let code = r#"
            trait Handler { fn handle(&self); }
            fn f(h: Box<dyn Handler>) {}
        "#;
        let f = analyze_fn(code, "f");
        // Box<dyn Handler> maps through Nullable → but convert_type_path
        // for Box extracts the first generic arg, which is a TraitObject → Opaque.
        // The actual mapping depends on how Box is handled: it may unwrap to Opaque directly.
        assert_eq!(
            f.params[0].typ,
            TypeInfo::Opaque {
                label: "dyn Handler".to_string()
            }
        );
    }

    // ── File context extraction tests ──

    fn parse_context(code: &str) -> FileContext {
        let file = syn::parse_file(code).expect("should parse");
        collect_file_context(&file)
    }

    #[test]
    fn file_context_extracts_simple_use_paths() {
        let ctx = parse_context("use tokio::spawn;\nuse axum::Json;\n");
        assert!(ctx.use_paths.contains(&"tokio::spawn".to_string()));
        assert!(ctx.use_paths.contains(&"axum::Json".to_string()));
    }

    #[test]
    fn file_context_extracts_grouped_use() {
        let ctx = parse_context("use tokio::{spawn, sync::Mutex};\n");
        assert!(ctx.use_paths.contains(&"tokio::spawn".to_string()));
        assert!(ctx.use_paths.contains(&"tokio::sync::Mutex".to_string()));
    }

    #[test]
    fn file_context_extracts_glob_use() {
        let ctx = parse_context("use tokio::*;\n");
        assert!(ctx.use_paths.contains(&"tokio::*".to_string()));
    }

    #[test]
    fn file_context_extracts_rename_use() {
        let ctx = parse_context("use axum::extract::Json as AxumJson;\n");
        assert!(ctx.use_paths.contains(&"axum::extract::Json".to_string()));
    }

    #[test]
    fn file_context_detects_tokio_main() {
        let ctx = parse_context("#[tokio::main]\nasync fn main() {}\n");
        assert!(ctx.has_tokio_macro);
    }

    #[test]
    fn file_context_detects_tokio_test() {
        let ctx = parse_context("#[tokio::test]\nasync fn test_it() {}\n");
        assert!(ctx.has_tokio_macro);
    }

    #[test]
    fn file_context_detects_tokio_main_with_args() {
        let ctx = parse_context("#[tokio::main(flavor = \"multi_thread\")]\nasync fn main() {}\n");
        assert!(ctx.has_tokio_macro);
    }

    #[test]
    fn file_context_no_tokio_macro_for_other_attrs() {
        let ctx = parse_context("#[test]\nfn test_it() {}\n");
        assert!(!ctx.has_tokio_macro);
    }

    #[test]
    fn analyze_source_with_context_returns_both() {
        let code = "use tokio::spawn;\npub async fn foo(x: i32) -> i32 { x }\n";
        let (fns, ctx) = analyze_source_with_context(code, None).expect("should succeed");
        assert_eq!(fns.len(), 1);
        assert!(fns[0].is_async);
        assert!(ctx.use_paths.contains(&"tokio::spawn".to_string()));
    }
}
