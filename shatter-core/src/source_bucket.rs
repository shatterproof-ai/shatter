//! Path-based source-set classifier (str-jeen.37).
//!
//! Each file the scan touches is assigned to exactly one [`SourceBucket`]
//! before any AST-level work runs. The classifier looks only at the path
//! string — file extension, directory components, and well-known naming
//! conventions across TypeScript, Go, and Rust ecosystems. It never opens
//! the file, parses it, or consults a frontend.
//!
//! # Precedence
//!
//! When a path matches multiple rules the higher-precedence bucket wins,
//! in this order (highest first):
//!
//! 1. [`SourceBucket::PolicyExcluded`] — vendored/build-output/VCS dirs
//!    that should never be reported on regardless of their contents.
//! 2. [`SourceBucket::Generated`] — auto-generated code (`*.pb.go`,
//!    `*_pb.ts`, `*.gen.*`, `**/generated/**`). Filtered out before
//!    grading because exercising generated code yields no signal about
//!    human-authored behavior.
//! 3. [`SourceBucket::Unsupported`] — files Shatter has no frontend for
//!    (str-jeen.47): the path's extension is not in the supported
//!    allowlist (TS/Go/Rust frontends accept `.ts`, `.tsx`, `.js`,
//!    `.jsx`, `.mjs`, `.cjs`, `.d.ts`, `.go`, `.rs`), or the basename
//!    is a known build/config artifact (`Makefile`, `Dockerfile`,
//!    `Cargo.toml`, `package.json`, etc.). Excluded from the coverage
//!    denominator so unanalyzable file types don't deflate "% attempted".
//! 4. [`SourceBucket::DeclarationOnly`] — type-only files (`*.d.ts`)
//!    with no executable bodies.
//! 5. [`SourceBucket::FixtureSample`] — corpora the project ships as
//!    test inputs or reference samples (`testdata/`, `fixtures/`,
//!    `examples/`, `samples/`). They live alongside source but are
//!    consumed by tests, not exercised directly.
//! 6. [`SourceBucket::TestSpec`] — test files (`*_test.go`,
//!    `*.test.ts`, `*.spec.ts`, `tests/`, `__tests__/`).
//! 7. [`SourceBucket::ProductionIsh`] — the default for human-authored
//!    source that doesn't match a more specific bucket.
//!
//! Higher-precedence buckets shadow lower ones on conflict — e.g. a
//! `_test.go` file inside `testdata/` is `FixtureSample`, not
//! `TestSpec`, because `testdata/` is the more meaningful classification
//! for that path.
//!
//! # Why no shebang detection (str-f3fd)
//!
//! The original str-jeen.47 spec listed shebang inspection
//! (`#!/bin/bash`, `#!/usr/bin/env python`) as a third detection signal
//! for routing extensionless executables. str-f3fd revisited that and
//! decided **not** to add shebang detection. The path-only contract
//! ("never opens the file") is intentional, not a TODO.
//!
//! The reasoning: an extensionless executable like `tools/release-cut`
//! already classifies as [`SourceBucket::Unsupported`] today, because
//! [`is_supported_extension`] returns `false` for any basename that
//! doesn't end in one of [`SUPPORTED_EXTENSIONS`], and the
//! `Unsupported` check sits above `ProductionIsh` in the precedence
//! order. Reading the file's first line to discover a `#!python` or
//! `#!bash` shebang would not change the bucket: Shatter has no Python,
//! Bash, or Ruby frontend, so any script the shebang identifies is
//! still unanalyzable and still belongs in `Unsupported`. The only
//! information shebang detection would add is a finer-grained label
//! ("python script" vs "unknown extensionless blob") that nothing in
//! the coverage denominator or the markdown source-set summary
//! consumes.
//!
//! Adding a content-read budget to the classifier would also break the
//! "path-only, no I/O" property the rest of the codebase relies on
//! (cheap to call, deterministic from the path string, safe to fold
//! over a file list without touching the filesystem). If a future
//! frontend covers a language whose source files are conventionally
//! extensionless and shebang-tagged, revisit this decision then —
//! introduce the shebang signal alongside the new frontend in the same
//! change so the I/O cost buys an actual bucket movement.

use serde::{Deserialize, Serialize};

/// Classification of a single source path's role in the codebase.
///
/// Serialized as snake_case strings in the scan report JSON. Each file
/// reported by a scan carries exactly one bucket value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceBucket {
    /// Human-authored source files that don't match a more specific
    /// bucket. The default classification.
    #[default]
    ProductionIsh,
    /// Test or spec files (`*_test.go`, `*.test.ts`, `*.spec.ts`,
    /// `tests/`, `__tests__/`).
    TestSpec,
    /// Auto-generated code (`*.pb.go`, `*_pb.ts`, `*.gen.*`,
    /// `**/generated/**`, `**/codegen/**`).
    Generated,
    /// Type-only declaration files (`*.d.ts`) with no executable bodies.
    DeclarationOnly,
    /// Test fixtures, sample corpora, or example inputs (`testdata/`,
    /// `fixtures/`, `examples/`, `samples/`).
    FixtureSample,
    /// Vendored, build-output, or VCS directories that policy excludes
    /// from reporting (`vendor/`, `node_modules/`, `target/`, `dist/`,
    /// `build/`, `.git/`).
    PolicyExcluded,
    /// Files Shatter has no frontend for (str-jeen.47): shell scripts,
    /// Python/Ruby sources, YAML/TOML/JSON configs, Markdown, `.proto`,
    /// `.sql`, `.css`, `.html`, and well-known build/config filenames
    /// like `Makefile`, `Dockerfile`, `Cargo.toml`, `package.json`,
    /// `go.mod`. Excluded from the coverage denominator so the
    /// "% attempted" number reflects only files Shatter could
    /// structurally analyze. Surfaced as its own row in the markdown
    /// source-set summary table (str-jeen.39) so the gap between "all
    /// files" and "denominator" is visible.
    Unsupported,
}

impl SourceBucket {
    /// Stable wire string used in JSON output. Matches the serde
    /// `rename_all = "snake_case"` representation.
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::ProductionIsh => "production_ish",
            Self::TestSpec => "test_spec",
            Self::Generated => "generated",
            Self::DeclarationOnly => "declaration_only",
            Self::FixtureSample => "fixture_sample",
            Self::PolicyExcluded => "policy_excluded",
            Self::Unsupported => "unsupported",
        }
    }
}

/// Classify a source-file path into a [`SourceBucket`]. Path-only —
/// never reads file contents. See module docs for precedence rules.
///
/// Paths whose extension is not in the supported allowlist (TS/Go/Rust
/// frontends) or whose basename is a known build/config filename
/// classify as [`SourceBucket::Unsupported`]. Empty paths classify as
/// [`SourceBucket::Unsupported`] as well — there is no recognizable
/// frontend signal in an empty path.
///
/// Extensionless executables follow the same rule: a path like
/// `tools/release-cut` or `bin/deploy` returns [`SourceBucket::Unsupported`]
/// because its basename ends in no recognized extension. The classifier
/// does not inspect the file's first line for a shebang (`#!/bin/bash`,
/// `#!/usr/bin/env python`) — see the module-level "Why no shebang
/// detection" section for the rationale.
pub fn classify_path(path: &str) -> SourceBucket {
    let normalized = normalize_path(path);
    let lower = normalized.to_ascii_lowercase();
    let segments: Vec<&str> = lower.split('/').filter(|s| !s.is_empty()).collect();
    let basename = segments.last().copied().unwrap_or("");

    if is_policy_excluded(&segments) {
        return SourceBucket::PolicyExcluded;
    }
    if is_generated(basename, &segments) {
        return SourceBucket::Generated;
    }
    if !is_supported_extension(basename) {
        return SourceBucket::Unsupported;
    }
    if is_declaration_only(basename) {
        return SourceBucket::DeclarationOnly;
    }
    if is_fixture_sample(&segments) {
        return SourceBucket::FixtureSample;
    }
    if is_test_spec(basename, &segments) {
        return SourceBucket::TestSpec;
    }
    SourceBucket::ProductionIsh
}

/// File extensions Shatter's frontends actually accept. Anything else
/// classifies as [`SourceBucket::Unsupported`] — see str-jeen.47. The
/// list intentionally tracks the union of every frontend's accepted
/// extension set; adding a new frontend means adding here.
///
/// `.d.ts` is matched implicitly because every supported `.d.ts` path
/// also ends with `.ts`.
const SUPPORTED_EXTENSIONS: &[&str] = &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".go", ".rs"];

/// Return whether `basename` ends in one of [`SUPPORTED_EXTENSIONS`].
/// A basename with no recognized extension (including extensionless
/// build-system filenames like `Makefile` and the empty string) returns
/// `false` and is classified as [`SourceBucket::Unsupported`] by
/// [`classify_path`].
fn is_supported_extension(basename: &str) -> bool {
    SUPPORTED_EXTENSIONS
        .iter()
        .any(|ext| basename.ends_with(ext))
}

/// Replace backslashes with forward slashes so Windows-style paths in
/// stored reports classify the same way as POSIX paths.
fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

/// Directory names that policy unconditionally excludes from reporting.
const POLICY_EXCLUDED_DIRS: &[&str] = &[
    "node_modules",
    "vendor",
    "target",
    "dist",
    "build",
    "out",
    ".git",
    ".svn",
    ".hg",
    ".next",
    ".nuxt",
    "coverage",
    "bazel-out",
    "bazel-bin",
];

fn is_policy_excluded(segments: &[&str]) -> bool {
    segments.iter().any(|s| POLICY_EXCLUDED_DIRS.contains(s))
}

/// Directory names whose contents are auto-generated.
const GENERATED_DIRS: &[&str] = &["generated", "codegen", "__generated__", "gen"];

/// Filename suffixes that mark generated code. Checked against the
/// lowercased basename.
const GENERATED_SUFFIXES: &[&str] = &[
    ".pb.go",
    ".pb.gw.go",
    "_pb.go",
    "_pb.ts",
    "_pb.js",
    "_pb.d.ts",
    ".gen.go",
    ".gen.ts",
    ".gen.tsx",
    ".gen.js",
    ".gen.rs",
    "_generated.go",
    "_generated.ts",
    "_generated.rs",
];

fn is_generated(basename: &str, segments: &[&str]) -> bool {
    if segments.iter().any(|s| GENERATED_DIRS.contains(s)) {
        return true;
    }
    GENERATED_SUFFIXES.iter().any(|sfx| basename.ends_with(sfx))
}

fn is_declaration_only(basename: &str) -> bool {
    basename.ends_with(".d.ts")
}

/// Directory names that hold test fixtures or example corpora.
const FIXTURE_DIRS: &[&str] = &[
    "testdata",
    "test-data",
    "fixtures",
    "fixture",
    "examples",
    "example",
    "samples",
    "sample",
    "__fixtures__",
];

fn is_fixture_sample(segments: &[&str]) -> bool {
    segments.iter().any(|s| FIXTURE_DIRS.contains(s))
}

/// Directory names that hold test files. `testdata` is intentionally
/// absent — see [`FIXTURE_DIRS`] and the precedence rules in the module
/// docs. `spec` and `specs` are intentionally absent: those names are used
/// for API-specification directories in Go and other languages; supported
/// test conventions are all filename-suffix-based (e.g. `_test.go`,
/// `.spec.ts`) and covered by [`TEST_SUFFIXES`].
const TEST_DIRS: &[&str] = &["__tests__", "tests", "test"];

/// Filename suffixes that mark test or spec files.
const TEST_SUFFIXES: &[&str] = &[
    "_test.go",
    "_test.rs",
    ".test.ts",
    ".test.tsx",
    ".test.js",
    ".test.jsx",
    ".test.mjs",
    ".spec.ts",
    ".spec.tsx",
    ".spec.js",
    ".spec.jsx",
    ".spec.mjs",
];

fn is_test_spec(basename: &str, segments: &[&str]) -> bool {
    if TEST_SUFFIXES.iter().any(|sfx| basename.ends_with(sfx)) {
        return true;
    }
    // Only treat a directory as a test directory when it isn't the
    // basename — `tests.go` is a regular file, but `tests/foo.go` is a
    // test path.
    let dir_segments = if segments.is_empty() {
        &[][..]
    } else {
        &segments[..segments.len() - 1]
    };
    dir_segments.iter().any(|s| TEST_DIRS.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_production_source() {
        assert_eq!(
            classify_path("src/app/handler.ts"),
            SourceBucket::ProductionIsh
        );
        assert_eq!(
            classify_path("pkg/server/server.go"),
            SourceBucket::ProductionIsh
        );
        assert_eq!(
            classify_path("crates/core/src/lib.rs"),
            SourceBucket::ProductionIsh
        );
        // Empty paths have no extension signal — they classify as
        // Unsupported (str-jeen.47), not ProductionIsh.
        assert_eq!(classify_path(""), SourceBucket::Unsupported);
    }

    #[test]
    fn classifies_test_spec_files() {
        assert_eq!(
            classify_path("pkg/server/server_test.go"),
            SourceBucket::TestSpec
        );
        assert_eq!(
            classify_path("src/app/handler.test.ts"),
            SourceBucket::TestSpec
        );
        assert_eq!(
            classify_path("src/app/handler.spec.tsx"),
            SourceBucket::TestSpec
        );
        assert_eq!(
            classify_path("src/__tests__/util.ts"),
            SourceBucket::TestSpec
        );
        assert_eq!(
            classify_path("tests/integration/run.go"),
            SourceBucket::TestSpec
        );
    }

    #[test]
    fn specs_dir_not_classified_as_test_spec() {
        // Production files in an `internal/specs` or `pkg/specs` directory
        // must not be mislabelled as TestSpec (str-9awj).
        assert_eq!(
            classify_path("internal/specs/loader.go"),
            SourceBucket::ProductionIsh
        );
        assert_eq!(
            classify_path("pkg/specs/openapi.go"),
            SourceBucket::ProductionIsh
        );
        // A _test.go file inside a specs/ directory is still TestSpec via suffix.
        assert_eq!(
            classify_path("internal/specs/loader_test.go"),
            SourceBucket::TestSpec
        );
        // The tests/ directory marker continues to work.
        assert_eq!(
            classify_path("tests/integration/run.go"),
            SourceBucket::TestSpec
        );
    }

    #[test]
    fn classifies_generated_code() {
        assert_eq!(
            classify_path("api/v1/service.pb.go"),
            SourceBucket::Generated
        );
        assert_eq!(classify_path("proto/foo_pb.ts"), SourceBucket::Generated);
        assert_eq!(classify_path("src/schema.gen.ts"), SourceBucket::Generated);
        assert_eq!(
            classify_path("internal/generated/wire.go"),
            SourceBucket::Generated
        );
        assert_eq!(
            classify_path("pkg/codegen/output.go"),
            SourceBucket::Generated
        );
        assert_eq!(
            classify_path("src/__generated__/types.ts"),
            SourceBucket::Generated
        );
    }

    #[test]
    fn classifies_declaration_only() {
        assert_eq!(
            classify_path("types/global.d.ts"),
            SourceBucket::DeclarationOnly
        );
        assert_eq!(
            classify_path("node_modules/@types/node/index.d.ts"),
            // policy_excluded outranks declaration_only
            SourceBucket::PolicyExcluded
        );
    }

    #[test]
    fn classifies_fixture_sample() {
        assert_eq!(
            classify_path("pkg/parser/testdata/input.go"),
            SourceBucket::FixtureSample
        );
        assert_eq!(
            classify_path("examples/go/04-nested.go"),
            SourceBucket::FixtureSample
        );
        assert_eq!(
            classify_path("src/__fixtures__/sample.ts"),
            SourceBucket::FixtureSample
        );
        // testdata wins over `_test.go` suffix
        assert_eq!(
            classify_path("pkg/parser/testdata/some_test.go"),
            SourceBucket::FixtureSample
        );
    }

    #[test]
    fn classifies_policy_excluded() {
        assert_eq!(
            classify_path("node_modules/foo/index.js"),
            SourceBucket::PolicyExcluded
        );
        assert_eq!(
            classify_path("vendor/github.com/foo/bar.go"),
            SourceBucket::PolicyExcluded
        );
        assert_eq!(
            classify_path("target/debug/build/x.rs"),
            SourceBucket::PolicyExcluded
        );
        assert_eq!(classify_path("dist/main.js"), SourceBucket::PolicyExcluded);
        assert_eq!(
            classify_path(".git/hooks/pre-commit"),
            SourceBucket::PolicyExcluded
        );
    }

    #[test]
    fn windows_paths_classify_the_same_as_posix() {
        assert_eq!(
            classify_path(r"pkg\server\server_test.go"),
            SourceBucket::TestSpec
        );
        assert_eq!(
            classify_path(r"node_modules\foo\index.js"),
            SourceBucket::PolicyExcluded
        );
    }

    #[test]
    fn precedence_order_is_correct() {
        // policy_excluded > generated
        assert_eq!(
            classify_path("vendor/foo/types_pb.go"),
            SourceBucket::PolicyExcluded
        );
        // generated > declaration_only
        assert_eq!(
            classify_path("src/generated/types.d.ts"),
            SourceBucket::Generated
        );
        // generated > test_spec
        assert_eq!(
            classify_path("src/generated/foo_test.go"),
            SourceBucket::Generated
        );
        // fixture_sample > test_spec
        assert_eq!(
            classify_path("examples/foo.test.ts"),
            SourceBucket::FixtureSample
        );
    }

    #[test]
    fn classifies_unsupported_extensions_and_filenames() {
        // Unsupported extensions enumerated in str-jeen.47 acceptance
        // criteria.
        let unsupported_extension_paths = [
            "scripts/build.sh",
            "scripts/run.bash",
            "scripts/build.py",
            "scripts/lint.rb",
            ".github/workflows/ci.yaml",
            ".github/workflows/ci.yml",
            "pyproject.toml",
            "package.json",
            "README.md",
            "proto/api.proto",
            "db/schema.sql",
            "src/app.css",
            "src/index.html",
            "deploy/web.dockerfile",
        ];
        for path in unsupported_extension_paths {
            assert_eq!(
                classify_path(path),
                SourceBucket::Unsupported,
                "expected Unsupported for {path}",
            );
        }

        // Build/config filenames without supported extensions also
        // classify as Unsupported.
        let unsupported_filename_paths = [
            "Makefile",
            "Dockerfile",
            "Containerfile",
            "Justfile",
            "BUILD.bazel",
        ];
        for path in unsupported_filename_paths {
            assert_eq!(
                classify_path(path),
                SourceBucket::Unsupported,
                "expected Unsupported for {path}",
            );
        }
    }

    #[test]
    fn supported_extension_allowlist_boundary() {
        // Every extension a frontend accepts must classify out of
        // Unsupported (the file may still be Test/Fixture/etc; the
        // boundary check is "not Unsupported").
        let supported_paths = [
            "src/app.ts",
            "src/Card.tsx",
            "src/util.js",
            "src/Card.jsx",
            "src/loader.mjs",
            "src/loader.cjs",
            "types/global.d.ts",
            "pkg/server.go",
            "crates/core/src/lib.rs",
        ];
        for path in supported_paths {
            assert_ne!(
                classify_path(path),
                SourceBucket::Unsupported,
                "supported extension misclassified as Unsupported: {path}",
            );
        }

        // Unsupported neighbors of the allowlist boundary.
        let just_outside = ["src/app.zig", "src/lib.kt", "src/main.swift"];
        for path in just_outside {
            assert_eq!(
                classify_path(path),
                SourceBucket::Unsupported,
                "expected Unsupported for {path}",
            );
        }
    }

    #[test]
    fn unsupported_precedence() {
        // policy_excluded wins over Unsupported.
        assert_eq!(
            classify_path("node_modules/foo/package.json"),
            SourceBucket::PolicyExcluded,
        );
        // Generated wins over Unsupported (a file under generated/ with
        // an unsupported extension is still Generated).
        assert_eq!(
            classify_path("internal/generated/schema.json"),
            SourceBucket::Generated,
        );
        // Unsupported wins over fixture/test directory hints when the
        // file's own extension is not analyzable.
        assert_eq!(
            classify_path("examples/python/demo.py"),
            SourceBucket::Unsupported,
        );
        assert_eq!(
            classify_path("tests/integration/run.py"),
            SourceBucket::Unsupported,
        );
    }

    #[test]
    fn extensionless_executables_classify_as_unsupported() {
        // str-f3fd: extensionless interpreted scripts (typically
        // shebang-tagged: `#!/bin/bash`, `#!/usr/bin/env python`)
        // classify as `Unsupported` via the supported-extension gate
        // alone, without the classifier ever reading file contents.
        // Locking this in as a regression test so a future change that
        // accidentally promotes extensionless paths to `ProductionIsh`
        // (e.g. moving the `is_supported_extension` check below the
        // default) fails loudly.
        // Avoid basenames that collide with `POLICY_EXCLUDED_DIRS`
        // (`build`, `dist`, `out`, ...) — those would classify as
        // `PolicyExcluded` by the directory-name rule, not by the
        // supported-extension gate, and would mask the property under
        // test.
        let extensionless_executable_paths = [
            "tools/release-cut",
            "bin/deploy",
            "scripts/bootstrap",
            "hooks/pre-commit",
        ];
        for path in extensionless_executable_paths {
            assert_eq!(
                classify_path(path),
                SourceBucket::Unsupported,
                "expected Unsupported for extensionless executable {path}",
            );
        }
    }

    #[test]
    fn wire_strings_match_serde_output() {
        let pairs = [
            (SourceBucket::ProductionIsh, "production_ish"),
            (SourceBucket::TestSpec, "test_spec"),
            (SourceBucket::Generated, "generated"),
            (SourceBucket::DeclarationOnly, "declaration_only"),
            (SourceBucket::FixtureSample, "fixture_sample"),
            (SourceBucket::PolicyExcluded, "policy_excluded"),
            (SourceBucket::Unsupported, "unsupported"),
        ];
        for (bucket, wire) in pairs {
            assert_eq!(bucket.as_wire_str(), wire);
            let json = serde_json::to_string(&bucket).expect("serialize");
            assert_eq!(json, format!("\"{wire}\""));
            let round: SourceBucket = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(round, bucket);
        }
    }
}
