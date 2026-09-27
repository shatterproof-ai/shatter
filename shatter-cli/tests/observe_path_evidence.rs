//! str-k2dny.4: `shatter observe --path-predicate-bundle/--path-evidence-provenance`
//! folds a real TypeScript exploration into an existing path predicate bundle.
//!
//! Every raw result is counted exactly once by a predicate with the asserted
//! target (as eligible or not-applicable), so the bundle's evidence total must
//! equal the `raw_results` length of the same run's observation JSON, in both
//! random and concolic modes.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use shatter_core::invariants::{
    InputPath, InputPathKind, ObservationPoint, PATH_PREDICATE_SCHEMA_VERSION,
    PATH_PREDICATE_SEMANTICS_VERSION, PathCompareOp, PathExpression, PathPredicateRecord,
    PathPredicateScope, PathPredicateTarget, PredicateEvidence, PredicateLifecycle,
    canonical_path_predicate_id,
};
use shatter_core::path_predicate_store::{
    PATH_PREDICATE_BUNDLE_SCHEMA_VERSION, PathPredicateBundle, read_path_predicate_bundle,
    write_path_predicate_bundle,
};

const SOURCE: &str = "export function compare(a: number, b: number): number {\n  \
    if (a < b) {\n    return -1;\n  }\n  return 1;\n}\n";

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new(function_name: &str, frontend: &str) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("compare.ts"), SOURCE).expect("write source");
        let target = PathPredicateTarget {
            qualified_function: function_name.into(),
            frontend: frontend.into(),
            source_fingerprint: "source-v1".into(),
        };
        let operand = |parameter| InputPath {
            kind: InputPathKind::InputPath,
            parameter,
            path: vec![],
        };
        let mut record = PathPredicateRecord {
            schema_version: PATH_PREDICATE_SCHEMA_VERSION,
            predicate_id: String::new(),
            target,
            scope: PathPredicateScope {
                path_prefix: vec![(0, true)],
                observation_point: ObservationPoint::Entry,
                context_fingerprint: "context-v1".into(),
            },
            expression: PathExpression::Compare {
                op: PathCompareOp::Lt,
                left: operand(0),
                right: operand(1),
            },
            semantics_version: PATH_PREDICATE_SEMANTICS_VERSION.into(),
            lifecycle: PredicateLifecycle::Candidate,
            evidence: PredicateEvidence::default(),
        };
        record.predicate_id = canonical_path_predicate_id(&record).expect("ID");
        write_path_predicate_bundle(
            &dir.path().join("bundle.json"),
            &PathPredicateBundle {
                schema_version: PATH_PREDICATE_BUNDLE_SCHEMA_VERSION,
                predicates: vec![record],
            },
        )
        .expect("write bundle");
        std::fs::write(
            dir.path().join("provenance.json"),
            format!(
                r#"{{"target":{{"qualified_function":"{function_name}","frontend":"{frontend}","source_fingerprint":"source-v1"}},"context_fingerprint":"context-v1"}}"#
            ),
        )
        .expect("write provenance");
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    fn bundle_bytes(&self) -> Vec<u8> {
        std::fs::read(self.path("bundle.json")).expect("read bundle")
    }

    fn observe(&self, extra: &[&str]) -> Output {
        let target = format!("{}:compare", self.path("compare.ts").display());
        let tmp = tempfile::tempdir().expect("command tmpdir");
        Command::new(env!("CARGO_BIN_EXE_shatter"))
            .env("SHATTER_ALLOW_HOST_WRITES", "1")
            .env("TMPDIR", tmp.path())
            .args(["observe", &target, "--max-iterations", "10"])
            .args(extra)
            .output()
            .expect("invoke shatter observe")
    }

    fn evidence_flags(&self) -> Vec<String> {
        vec![
            "--path-predicate-bundle".into(),
            self.path("bundle.json").display().to_string(),
            "--path-evidence-provenance".into(),
            self.path("provenance.json").display().to_string(),
        ]
    }

    fn observe_with_evidence(&self, extra: &[&str]) -> Output {
        let flags = self.evidence_flags();
        let mut args: Vec<&str> = flags.iter().map(String::as_str).collect();
        args.extend_from_slice(extra);
        self.observe(&args)
    }
}

fn evidence_total(path: &Path) -> u64 {
    let bundle = read_path_predicate_bundle(path).expect("bundle readable");
    let evidence = &bundle.predicates[0].evidence;
    assert!(evidence.supporting_witnesses.is_empty(), "invented witness");
    assert!(evidence.refuting_witnesses.is_empty(), "invented witness");
    u64::from(evidence.eligible + evidence.not_applicable)
}

fn raw_result_count(stdout: &[u8]) -> u64 {
    let json: serde_json::Value = serde_json::from_slice(stdout).expect("observe JSON on stdout");
    json["observation"]["raw_results"]
        .as_array()
        .expect("raw_results array")
        .len() as u64
}

fn assert_collects(extra: &[&str]) {
    let fixture = Fixture::new("compare", "typescript");
    let output = fixture.observe_with_evidence(extra);
    assert!(
        output.status.success(),
        "observe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let raw = raw_result_count(&output.stdout);
    assert!(raw > 0, "exploration produced no raw results");
    assert_eq!(evidence_total(&fixture.path("bundle.json")), raw);
}

#[test]
fn random_observe_records_evidence() {
    assert_collects(&[]);
}

#[test]
fn concolic_observe_records_evidence() {
    assert_collects(&["--concolic"]);
}

#[test]
fn opt_out_observe_leaves_bundle_untouched() {
    let fixture = Fixture::new("compare", "typescript");
    let before = fixture.bundle_bytes();
    let output = fixture.observe(&[]);
    assert!(output.status.success());
    assert!(raw_result_count(&output.stdout) > 0);
    assert_eq!(fixture.bundle_bytes(), before);
}

#[test]
fn unpaired_flags_are_rejected() {
    let fixture = Fixture::new("compare", "typescript");
    let bundle = fixture.path("bundle.json").display().to_string();
    let provenance = fixture.path("provenance.json").display().to_string();
    for args in [
        ["--path-predicate-bundle", bundle.as_str()],
        ["--path-evidence-provenance", provenance.as_str()],
    ] {
        let before = fixture.bundle_bytes();
        let output = fixture.observe(&args);
        assert!(!output.status.success(), "accepted lone {}", args[0]);
        assert!(output.stdout.is_empty());
        assert_eq!(fixture.bundle_bytes(), before);
    }
}

#[test]
fn invalid_provenance_and_bindings_fail_without_changing_bundle() {
    // Analyze names this function "compare"; the qualified name differs.
    let name_mismatch = Fixture::new("example.compare", "typescript");
    let frontend_mismatch = Fixture::new("compare", "go");
    let malformed = Fixture::new("compare", "typescript");
    std::fs::write(malformed.path("provenance.json"), "{").expect("corrupt provenance");
    let empty_context = Fixture::new("compare", "typescript");
    std::fs::write(
        empty_context.path("provenance.json"),
        r#"{"target":{"qualified_function":"compare","frontend":"typescript","source_fingerprint":"source-v1"},"context_fingerprint":""}"#,
    )
    .expect("empty context provenance");
    let absent_target = Fixture::new("compare", "typescript");
    std::fs::write(
        absent_target.path("provenance.json"),
        r#"{"target":{"qualified_function":"compare","frontend":"typescript","source_fingerprint":"other"},"context_fingerprint":"context-v1"}"#,
    )
    .expect("absent target provenance");
    let missing_bundle = Fixture::new("compare", "typescript");
    std::fs::remove_file(missing_bundle.path("bundle.json")).expect("remove bundle");

    for fixture in [
        &name_mismatch,
        &frontend_mismatch,
        &malformed,
        &empty_context,
        &absent_target,
    ] {
        let before = fixture.bundle_bytes();
        let output = fixture.observe_with_evidence(&[]);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty(), "emitted observation JSON on failure");
        assert_eq!(fixture.bundle_bytes(), before);
    }
    let output = missing_bundle.observe_with_evidence(&[]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[cfg(unix)]
#[test]
fn write_failure_emits_no_observation_and_keeps_bundle() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("compare", "typescript");
    let bundle_dir = fixture.path("locked");
    std::fs::create_dir(&bundle_dir).expect("locked dir");
    std::fs::rename(fixture.path("bundle.json"), bundle_dir.join("bundle.json")).expect("move");
    let before = std::fs::read(bundle_dir.join("bundle.json")).expect("read");
    // Read-only directory: the bundle stays readable but the atomic sibling
    // file cannot be created. Skip when permissions are not enforced (root).
    std::fs::set_permissions(&bundle_dir, std::fs::Permissions::from_mode(0o555)).expect("chmod");
    if std::fs::File::create(bundle_dir.join("probe")).is_ok() {
        std::fs::set_permissions(&bundle_dir, std::fs::Permissions::from_mode(0o755)).ok();
        return;
    }
    let bundle = bundle_dir.join("bundle.json").display().to_string();
    let provenance = fixture.path("provenance.json").display().to_string();
    let output = fixture.observe(&[
        "--path-predicate-bundle",
        &bundle,
        "--path-evidence-provenance",
        &provenance,
    ]);
    std::fs::set_permissions(&bundle_dir, std::fs::Permissions::from_mode(0o755)).expect("restore");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read(bundle_dir.join("bundle.json")).expect("read"), before);
}
