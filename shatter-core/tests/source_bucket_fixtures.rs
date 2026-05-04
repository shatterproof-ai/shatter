//! Cross-language fixture coverage for [`shatter_core::source_bucket::classify_path`]
//! (str-jeen.38).
//!
//! The unit tests inside `source_bucket.rs` cover the precedence rules and a
//! handful of representative paths. This integration test is the **fixture
//! catalog**: a table-driven inventory of realistic paths from each ecosystem
//! Shatter targets — Go, TypeScript, TSX, Rust — plus the cross-cutting
//! categories the classifier must distinguish (generated code, test files,
//! fixtures/examples, declaration-only files, policy-excluded directories,
//! and config files).
//!
//! Each row pins the **current contract** of the classifier on a concrete
//! path. Fixture-only — this test must never be used to drive semantic
//! changes to the classifier. If a row's expected bucket starts to feel
//! wrong, that is a signal to file a follow-up issue against the classifier
//! semantics, not to flip the assertion in place.
//!
//! ## Config files
//!
//! Manifest and build-metadata files whose extensions are outside the
//! frontend allowlist (`Cargo.toml`, `package.json`, `tsconfig.json`,
//! `go.mod`, `Makefile`, etc.) classify as
//! [`SourceBucket::Unsupported`] (str-jeen.47) — they are excluded from
//! the coverage denominator because Shatter has no frontend that can
//! analyze them. Config files written in supported languages
//! (`vite.config.ts`, `.eslintrc.js`) keep their natural extension
//! classification (`ProductionIsh`) because they ARE analyzable.

use shatter_core::source_bucket::{classify_path, SourceBucket};

/// One classifier fixture: a realistic source path and the bucket the
/// classifier is contracted to assign to it.
struct Fixture {
    /// Path as it would appear in a scan report.
    path: &'static str,
    /// Bucket the classifier must return for `path`.
    expected: SourceBucket,
    /// Short label describing why this fixture exists. Surfaced in the
    /// failure message so a regression points at the intent, not just the
    /// path string.
    label: &'static str,
}

/// Run a slice of fixtures, collecting every mismatch before failing so a
/// single test run reports the full delta rather than the first failure.
fn check_fixtures(group: &str, fixtures: &[Fixture]) {
    let mut failures: Vec<String> = Vec::new();
    for fx in fixtures {
        let actual = classify_path(fx.path);
        if actual != fx.expected {
            failures.push(format!(
                "  [{group}] {label}: classify_path({path:?}) = {actual:?}, expected {expected:?}",
                group = group,
                label = fx.label,
                path = fx.path,
                actual = actual,
                expected = fx.expected,
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "classifier fixture mismatches in group '{group}':\n{}",
        failures.join("\n"),
    );
}

#[test]
fn go_fixtures_classify_correctly() {
    let fixtures = [
        Fixture {
            path: "cmd/server/main.go",
            expected: SourceBucket::ProductionIsh,
            label: "go production binary entrypoint",
        },
        Fixture {
            path: "pkg/router/router.go",
            expected: SourceBucket::ProductionIsh,
            label: "go production library file",
        },
        Fixture {
            path: "internal/auth/token.go",
            expected: SourceBucket::ProductionIsh,
            label: "go internal package",
        },
        Fixture {
            path: "pkg/router/router_test.go",
            expected: SourceBucket::TestSpec,
            label: "go _test.go suffix",
        },
        Fixture {
            path: "pkg/router/export_test.go",
            expected: SourceBucket::TestSpec,
            label: "go export_test pattern",
        },
        Fixture {
            path: "tests/integration/smoke.go",
            expected: SourceBucket::TestSpec,
            label: "go file under tests/ directory",
        },
        Fixture {
            path: "pkg/parser/testdata/golden.go",
            expected: SourceBucket::FixtureSample,
            label: "go testdata corpus file",
        },
        Fixture {
            path: "examples/go/04-nested-control-flow.go",
            expected: SourceBucket::FixtureSample,
            label: "go example program",
        },
    ];
    check_fixtures("go", &fixtures);
}

#[test]
fn typescript_fixtures_classify_correctly() {
    let fixtures = [
        Fixture {
            path: "src/index.ts",
            expected: SourceBucket::ProductionIsh,
            label: "ts production entrypoint",
        },
        Fixture {
            path: "src/util/string.ts",
            expected: SourceBucket::ProductionIsh,
            label: "ts production utility module",
        },
        Fixture {
            path: "src/util/string.test.ts",
            expected: SourceBucket::TestSpec,
            label: "ts colocated .test.ts file",
        },
        Fixture {
            path: "src/util/string.spec.ts",
            expected: SourceBucket::TestSpec,
            label: "ts colocated .spec.ts file",
        },
        Fixture {
            path: "src/__tests__/string.ts",
            expected: SourceBucket::TestSpec,
            label: "ts file under __tests__/ directory",
        },
        Fixture {
            path: "tests/e2e/login.ts",
            expected: SourceBucket::TestSpec,
            label: "ts file under tests/ directory",
        },
        Fixture {
            path: "src/__fixtures__/users.ts",
            expected: SourceBucket::FixtureSample,
            label: "ts file under __fixtures__/ directory",
        },
    ];
    check_fixtures("typescript", &fixtures);
}

#[test]
fn tsx_fixtures_classify_correctly() {
    let fixtures = [
        Fixture {
            path: "src/components/Button.tsx",
            expected: SourceBucket::ProductionIsh,
            label: "tsx production React component",
        },
        Fixture {
            path: "src/pages/Home.tsx",
            expected: SourceBucket::ProductionIsh,
            label: "tsx production page component",
        },
        Fixture {
            path: "src/components/Button.test.tsx",
            expected: SourceBucket::TestSpec,
            label: "tsx colocated .test.tsx file",
        },
        Fixture {
            path: "src/components/Button.spec.tsx",
            expected: SourceBucket::TestSpec,
            label: "tsx colocated .spec.tsx file",
        },
        Fixture {
            path: "src/__tests__/Button.tsx",
            expected: SourceBucket::TestSpec,
            label: "tsx file under __tests__/ directory",
        },
        Fixture {
            path: "examples/widgets/Demo.tsx",
            expected: SourceBucket::FixtureSample,
            label: "tsx example component",
        },
    ];
    check_fixtures("tsx", &fixtures);
}

#[test]
fn rust_fixtures_classify_correctly() {
    let fixtures = [
        Fixture {
            path: "crates/core/src/lib.rs",
            expected: SourceBucket::ProductionIsh,
            label: "rust crate library root",
        },
        Fixture {
            path: "crates/core/src/parser/mod.rs",
            expected: SourceBucket::ProductionIsh,
            label: "rust submodule file",
        },
        Fixture {
            path: "crates/cli/src/main.rs",
            expected: SourceBucket::ProductionIsh,
            label: "rust binary crate entrypoint",
        },
        Fixture {
            path: "crates/core/src/parser_test.rs",
            expected: SourceBucket::TestSpec,
            label: "rust _test.rs suffix",
        },
        Fixture {
            path: "crates/core/tests/integration.rs",
            expected: SourceBucket::TestSpec,
            label: "rust file under tests/ integration directory",
        },
        Fixture {
            path: "crates/core/tests/fixtures/sample.rs",
            expected: SourceBucket::FixtureSample,
            label: "rust fixtures/ subdirectory under tests/",
        },
        Fixture {
            path: "examples/hello.rs",
            expected: SourceBucket::FixtureSample,
            label: "rust cargo example",
        },
    ];
    check_fixtures("rust", &fixtures);
}

#[test]
fn generated_fixtures_classify_correctly() {
    let fixtures = [
        Fixture {
            path: "api/v1/service.pb.go",
            expected: SourceBucket::Generated,
            label: "go protobuf .pb.go output",
        },
        Fixture {
            path: "api/v1/service.pb.gw.go",
            expected: SourceBucket::Generated,
            label: "go grpc-gateway .pb.gw.go output",
        },
        Fixture {
            path: "internal/store/queries_generated.go",
            expected: SourceBucket::Generated,
            label: "go _generated.go suffix",
        },
        Fixture {
            path: "src/proto/user_pb.ts",
            expected: SourceBucket::Generated,
            label: "ts protobuf _pb.ts output",
        },
        Fixture {
            path: "src/proto/user_pb.d.ts",
            expected: SourceBucket::Generated,
            label: "ts protobuf _pb.d.ts output",
        },
        Fixture {
            path: "src/schema.gen.ts",
            expected: SourceBucket::Generated,
            label: "ts .gen.ts codegen output",
        },
        Fixture {
            path: "src/components/Icons.gen.tsx",
            expected: SourceBucket::Generated,
            label: "tsx .gen.tsx codegen output",
        },
        Fixture {
            path: "crates/core/src/bindings.gen.rs",
            expected: SourceBucket::Generated,
            label: "rust .gen.rs codegen output",
        },
        Fixture {
            path: "crates/core/src/wire_generated.rs",
            expected: SourceBucket::Generated,
            label: "rust _generated.rs suffix",
        },
        Fixture {
            path: "internal/generated/wire.go",
            expected: SourceBucket::Generated,
            label: "go file under generated/ directory",
        },
        Fixture {
            path: "src/__generated__/types.ts",
            expected: SourceBucket::Generated,
            label: "ts file under __generated__/ directory",
        },
        Fixture {
            path: "pkg/codegen/output.go",
            expected: SourceBucket::Generated,
            label: "go file under codegen/ directory",
        },
    ];
    check_fixtures("generated", &fixtures);
}

#[test]
fn test_file_fixtures_classify_correctly() {
    // Cross-language test-file inventory in one place. Per-language tests
    // above cover the same patterns alongside production paths; this group
    // exists so a regression in test-file detection surfaces as a single
    // group failure spanning every language.
    let fixtures = [
        Fixture {
            path: "pkg/server/server_test.go",
            expected: SourceBucket::TestSpec,
            label: "go _test.go",
        },
        Fixture {
            path: "src/util/parse.test.ts",
            expected: SourceBucket::TestSpec,
            label: "ts .test.ts",
        },
        Fixture {
            path: "src/util/parse.spec.ts",
            expected: SourceBucket::TestSpec,
            label: "ts .spec.ts",
        },
        Fixture {
            path: "src/components/Card.test.tsx",
            expected: SourceBucket::TestSpec,
            label: "tsx .test.tsx",
        },
        Fixture {
            path: "src/components/Card.spec.tsx",
            expected: SourceBucket::TestSpec,
            label: "tsx .spec.tsx",
        },
        Fixture {
            path: "src/util/parse.test.js",
            expected: SourceBucket::TestSpec,
            label: "js .test.js",
        },
        Fixture {
            path: "src/util/parse.test.mjs",
            expected: SourceBucket::TestSpec,
            label: "mjs .test.mjs",
        },
        Fixture {
            path: "crates/core/src/parser_test.rs",
            expected: SourceBucket::TestSpec,
            label: "rust _test.rs",
        },
        Fixture {
            path: "tests/integration/run.go",
            expected: SourceBucket::TestSpec,
            label: "tests/ directory marker",
        },
        Fixture {
            path: "spec/features/login.ts",
            expected: SourceBucket::TestSpec,
            label: "spec/ directory marker",
        },
    ];
    check_fixtures("test_files", &fixtures);
}

#[test]
fn declaration_only_fixtures_classify_correctly() {
    let fixtures = [
        Fixture {
            path: "types/global.d.ts",
            expected: SourceBucket::DeclarationOnly,
            label: "top-level ambient declaration",
        },
        Fixture {
            path: "src/types/window.d.ts",
            expected: SourceBucket::DeclarationOnly,
            label: "colocated declaration file",
        },
    ];
    check_fixtures("declaration_only", &fixtures);
}

#[test]
fn policy_excluded_fixtures_classify_correctly() {
    let fixtures = [
        Fixture {
            path: "node_modules/react/index.js",
            expected: SourceBucket::PolicyExcluded,
            label: "npm node_modules tree",
        },
        Fixture {
            path: "vendor/github.com/foo/bar.go",
            expected: SourceBucket::PolicyExcluded,
            label: "go vendor/ tree",
        },
        Fixture {
            path: "target/debug/build/foo.rs",
            expected: SourceBucket::PolicyExcluded,
            label: "rust cargo target/ output",
        },
        Fixture {
            path: "dist/app.js",
            expected: SourceBucket::PolicyExcluded,
            label: "js bundler dist/ output",
        },
        Fixture {
            path: "build/static/main.css",
            expected: SourceBucket::PolicyExcluded,
            label: "generic build/ output",
        },
        Fixture {
            path: ".next/server/pages.js",
            expected: SourceBucket::PolicyExcluded,
            label: "next.js .next cache",
        },
        Fixture {
            path: "coverage/lcov-report/index.html",
            expected: SourceBucket::PolicyExcluded,
            label: "coverage/ tooling output",
        },
        Fixture {
            path: ".git/HEAD",
            expected: SourceBucket::PolicyExcluded,
            label: "git metadata directory",
        },
    ];
    check_fixtures("policy_excluded", &fixtures);
}

#[test]
fn config_and_build_metadata_classify_as_unsupported() {
    // str-jeen.47: manifest and build-metadata files whose extensions
    // aren't in the frontend allowlist classify as `Unsupported` so they
    // don't deflate the "% attempted" denominator. This was previously
    // `config_file_fixtures_classify_as_production_ish` (str-jeen.38);
    // the rename and reclassification are deliberate.
    let fixtures = [
        Fixture {
            path: "Cargo.toml",
            expected: SourceBucket::Unsupported,
            label: "rust workspace manifest",
        },
        Fixture {
            path: "crates/core/Cargo.toml",
            expected: SourceBucket::Unsupported,
            label: "rust crate manifest",
        },
        Fixture {
            path: "package.json",
            expected: SourceBucket::Unsupported,
            label: "npm manifest",
        },
        Fixture {
            path: "tsconfig.json",
            expected: SourceBucket::Unsupported,
            label: "ts compiler config",
        },
        Fixture {
            path: "go.mod",
            expected: SourceBucket::Unsupported,
            label: "go module manifest",
        },
        Fixture {
            path: "go.sum",
            expected: SourceBucket::Unsupported,
            label: "go module checksum file",
        },
        Fixture {
            path: ".github/workflows/ci.yml",
            expected: SourceBucket::Unsupported,
            label: "github actions workflow",
        },
        Fixture {
            path: "Taskfile.yml",
            expected: SourceBucket::Unsupported,
            label: "taskfile",
        },
        Fixture {
            path: "Makefile",
            expected: SourceBucket::Unsupported,
            label: "make build manifest",
        },
        Fixture {
            path: "Dockerfile",
            expected: SourceBucket::Unsupported,
            label: "dockerfile",
        },
        Fixture {
            path: "BUILD.bazel",
            expected: SourceBucket::Unsupported,
            label: "bazel build target",
        },
    ];
    check_fixtures("config_files", &fixtures);
}

#[test]
fn config_files_in_supported_languages_classify_as_production_ish() {
    // Config files written in languages a frontend can analyze keep
    // their natural extension classification — they aren't `Unsupported`
    // because Shatter CAN run analysis on them.
    let fixtures = [
        Fixture {
            path: ".eslintrc.js",
            expected: SourceBucket::ProductionIsh,
            label: "eslint config in javascript",
        },
        Fixture {
            path: "vite.config.ts",
            expected: SourceBucket::ProductionIsh,
            label: "vite config in typescript",
        },
    ];
    check_fixtures("config_files_supported_lang", &fixtures);
}

#[test]
fn unsupported_source_fixtures_classify_correctly() {
    // str-jeen.47: explicit allowlist boundary. Files in non-frontend
    // languages or with non-source extensions classify as `Unsupported`
    // so the coverage denominator only counts files Shatter can
    // structurally analyze.
    let fixtures = [
        Fixture {
            path: "scripts/build.sh",
            expected: SourceBucket::Unsupported,
            label: "shell script extension",
        },
        Fixture {
            path: "scripts/run.bash",
            expected: SourceBucket::Unsupported,
            label: "bash script extension",
        },
        Fixture {
            path: "scripts/build.py",
            expected: SourceBucket::Unsupported,
            label: "python source",
        },
        Fixture {
            path: "scripts/lint.rb",
            expected: SourceBucket::Unsupported,
            label: "ruby source",
        },
        Fixture {
            path: "config/values.yaml",
            expected: SourceBucket::Unsupported,
            label: "yaml config",
        },
        Fixture {
            path: "README.md",
            expected: SourceBucket::Unsupported,
            label: "markdown documentation",
        },
        Fixture {
            path: "proto/api.proto",
            expected: SourceBucket::Unsupported,
            label: "protobuf schema",
        },
        Fixture {
            path: "db/schema.sql",
            expected: SourceBucket::Unsupported,
            label: "sql schema",
        },
        Fixture {
            path: "src/app.css",
            expected: SourceBucket::Unsupported,
            label: "css stylesheet",
        },
        Fixture {
            path: "public/index.html",
            expected: SourceBucket::Unsupported,
            label: "html document",
        },
        Fixture {
            path: "Containerfile",
            expected: SourceBucket::Unsupported,
            label: "container build manifest",
        },
        Fixture {
            path: "Justfile",
            expected: SourceBucket::Unsupported,
            label: "just task runner manifest",
        },
    ];
    check_fixtures("unsupported", &fixtures);
}

#[test]
fn precedence_holds_across_language_fixtures() {
    // Verifies that the precedence rules documented in `source_bucket.rs`
    // hold for realistic cross-language paths, not just the synthetic ones
    // in the unit tests.
    let fixtures = [
        Fixture {
            path: "node_modules/@grpc/grpc-js/build/src/index.d.ts",
            expected: SourceBucket::PolicyExcluded,
            label: "policy_excluded shadows declaration_only",
        },
        Fixture {
            path: "vendor/google.golang.org/api/foo.pb.go",
            expected: SourceBucket::PolicyExcluded,
            label: "policy_excluded shadows generated",
        },
        Fixture {
            path: "vendor/github.com/foo/bar_test.go",
            expected: SourceBucket::PolicyExcluded,
            label: "policy_excluded shadows test_spec",
        },
        Fixture {
            path: "src/__generated__/api.test.ts",
            expected: SourceBucket::Generated,
            label: "generated shadows test_spec",
        },
        Fixture {
            path: "src/generated/window.d.ts",
            expected: SourceBucket::Generated,
            label: "generated shadows declaration_only",
        },
        Fixture {
            path: "examples/typescript/app.test.ts",
            expected: SourceBucket::FixtureSample,
            label: "fixture_sample shadows test_spec",
        },
        Fixture {
            path: "pkg/parser/testdata/case_test.go",
            expected: SourceBucket::FixtureSample,
            label: "testdata shadows go _test.go suffix",
        },
    ];
    check_fixtures("precedence", &fixtures);
}
