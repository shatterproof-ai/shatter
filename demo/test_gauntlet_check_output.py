"""Tests for demo/gauntlet_check_output.py (str-jeen.59; str-qwua7.10 extends
coverage to 0%-coverage function blocks, lifecycle/scope-mismatch thrown-error
clusters, and allowlist expiry)."""

import json
import re
import subprocess
import sys
import textwrap
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

REPO_ROOT = Path(__file__).resolve().parent.parent
SCRIPT = REPO_ROOT / "demo" / "gauntlet_check_output.py"
ALLOWLIST = REPO_ROOT / "demo" / "gauntlet-scan-allowlist.yaml"


# Pinned so these tests don't turn red purely by date rollover once the live
# allowlist's entries pass their `expires` date. The real gate (walkthrough.sh
# / gauntlet.sh) never passes --today, so a stale entry still fails it.
PINNED_TODAY = "2026-09-24"


def run_helper(output_text: str, allowlist: Path = ALLOWLIST) -> subprocess.CompletedProcess:
    with TemporaryDirectory() as td:
        out = Path(td) / "out.txt"
        out.write_text(output_text)
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--allowlist", str(allowlist), "--output", str(out), "--step", "test", "--today", PINNED_TODAY],
            capture_output=True,
            text=True,
            check=False,
        )


def write_allowlist(td: str, body: str) -> Path:
    path = Path(td) / "allowlist.yaml"
    path.write_text(body)
    return path


FIXTURES = REPO_ROOT / "demo" / "fixtures" / "scan-json"
FAILED_JSON = FIXTURES / "failed.json"
INTERRUPTED_JSON = FIXTURES / "interrupted.json"

EMPTY_ALLOWLIST = "expected_failures: []\n"


def run_scan_json(
    scan_json: Path | None,
    allowlist_body: str = EMPTY_ALLOWLIST,
    extra_args: tuple[str, ...] = (),
) -> subprocess.CompletedProcess:
    """Run the checker against a scan `--format json` report (str-49drv.149)."""
    with TemporaryDirectory() as td:
        allowlist = write_allowlist(td, allowlist_body)
        out = Path(td) / "out.txt"
        out.write_text("")
        cmd = [sys.executable, str(SCRIPT), "--allowlist", str(allowlist), "--output", str(out), "--step", "test", "--today", PINNED_TODAY]
        if scan_json is not None:
            cmd += ["--scan-json", str(scan_json)]
        return subprocess.run(cmd + list(extra_args), capture_output=True, text=True, check=False)


ALLOW_SPIN = textwrap.dedent(
    """\
    expected_failures:
      - file: loop.ts
        function: spin
        tracker: str-49drv.149
        reason: fixture function that never terminates
        expires: "2099-01-01"
    """
)
ALLOW_BOTH = ALLOW_SPIN + textwrap.indent(
    textwrap.dedent(
        """\
        - file: loop.ts
          function: ok
          tracker: str-49drv.149
          reason: fixture function interrupted before it ran
          expires: "2099-01-01"
        """
    ),
    "  ",
)


class GauntletCheckScanJsonTest(unittest.TestCase):
    """Fixtures in demo/fixtures/scan-json/ are real `shatter scan --format
    json` reports; see regenerate.sh there for the exact commands."""

    def test_failed_function_flagged(self) -> None:
        result = run_scan_json(FAILED_JSON)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("spin", result.stdout)
        self.assertIn("timed out", result.stdout)
        # `ok` completed in that scan; it must never be reported.
        self.assertNotIn("`ok`", result.stdout)

    def test_allowlisted_failure_passes(self) -> None:
        result = run_scan_json(FAILED_JSON, ALLOW_SPIN)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout, "")

    def test_failure_allowlist_reason_must_match_when_given(self) -> None:
        body = ALLOW_SPIN.replace(
            "    reason: fixture", "    reason_contains: panicked\n    reason: fixture"
        )
        result = run_scan_json(FAILED_JSON, body)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("spin", result.stdout)

    def test_interrupted_functions_flagged(self) -> None:
        result = run_scan_json(INTERRUPTED_JSON)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("interrupted", result.stdout)
        self.assertIn("spin", result.stdout)
        self.assertIn("ok", result.stdout)

    def test_allowlisted_interrupted_functions_pass(self) -> None:
        result = run_scan_json(INTERRUPTED_JSON, ALLOW_BOTH)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_expected_interrupted_step_passes(self) -> None:
        result = run_scan_json(
            INTERRUPTED_JSON, extra_args=("--expect-interrupted", "step bounds wall-clock")
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_expected_interrupted_does_not_excuse_failures(self) -> None:
        result = run_scan_json(FAILED_JSON, extra_args=("--expect-interrupted", "x"))
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)

    def _run_nested_id_report(self, section: dict) -> subprocess.CompletedProcess:
        allow_nested = textwrap.dedent(
            """\
            expected_failures:
              - file: lib.rs
                function: method
                tracker: str-49drv.149
                reason: nested qualified id (Type::method)
                expires: "2099-01-01"
            """
        )
        with TemporaryDirectory() as td:
            report = Path(td) / "scan.json"
            report.write_text(json.dumps({"codebase": {"failed": [], "skipped_functions": [], **section}}))
            return run_scan_json(report, allow_nested)

    def test_nested_qualified_id_matches_file_allowlist_for_interrupted(self) -> None:
        # `/x/src/lib.rs::Type::method`: the file is the text before the FIRST
        # `::`; splitting on the last one yields `lib.rs::Type` and never matches.
        result = self._run_nested_id_report(
            {
                "skipped_functions": [
                    {
                        "function_name": "method",
                        "category": "interrupted",
                        "qualified_id": "/x/src/lib.rs::Type::method",
                        "reason": "interrupted",
                    }
                ]
            }
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_nested_qualified_id_matches_file_allowlist_for_failed_without_file_path(self) -> None:
        result = self._run_nested_id_report(
            {
                "failed": [
                    {
                        "function_name": "method",
                        "qualified_id": "/x/src/lib.rs::Type::method",
                        "reason": "timed out",
                    }
                ]
            }
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_missing_scan_json_is_an_error(self) -> None:
        result = run_scan_json(FIXTURES / "does-not-exist.json")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("scan JSON", result.stdout)

    def test_unparseable_scan_json_is_an_error(self) -> None:
        with TemporaryDirectory() as td:
            bad = Path(td) / "bad.json"
            bad.write_text("not json")
            result = run_scan_json(bad)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("scan JSON", result.stdout)

    def test_no_scan_json_step_is_skipped_with_logged_reason(self) -> None:
        result = run_scan_json(None, extra_args=("--no-scan-json", "dry-run executes nothing"))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("dry-run executes nothing", result.stderr)

    def test_legacy_markdown_summary_is_no_longer_parsed(self) -> None:
        # The checker consumes JSON now; prose summaries carry no verdict.
        text = "Scan complete: **43 function(s)** tested, **0 skipped**, **9 error(s)**\n"
        self.assertEqual(run_helper(text).returncode, 0)


# Captured shape of the walkthrough's Rust step under the analyzer/harness
# param-type disagreement (str-qwua7.14): the CLI's explore_fn.md markdown
# (shatter-cli/templates/explore_fn.md), 0% coverage, every call throwing the
# real executor.rs input-deserialization message (shatter-rust/src/
# executor.rs:2471 etc: "input {i} deserialization failed: {err}"). This
# exact shape (function `syntheticZeroCovFn` is not a real example — chosen
# so this fixture can't collide with a live allowlist entry) was
# live-reproduced 2026-09-24 via `task affected` on this branch, with a
# fresh examples checkout, against classify_string/negotiate_language (see
# demo/gauntlet-scan-allowlist.yaml's str-qwua7.10 entries and this file's
# GauntletCheckOutputLiveRegressionTest for the real captured text).
RUST_ZERO_COVERAGE_EXCERPT = textwrap.dedent(
    """\
    ## `syntheticZeroCovFn` *(/tmp/shatter-examples-main/standalone/rust/01_arithmetic.rs:6-18)*

    **3 path(s)** · **0%** coverage (0/13 lines)

    | # | Call | Outcome |
    |---|---|---|
    | 1 | `syntheticZeroCovFn(0)` | throws `runtime_error: input 0 deserialization failed: invalid type: integer 0, expected a string` |
    | 2 | `syntheticZeroCovFn(-5)` | throws `runtime_error: input 0 deserialization failed: invalid type: integer -5, expected a string` |
    | 3 | `syntheticZeroCovFn(2)` | throws `runtime_error: input 0 deserialization failed: invalid type: integer 2, expected a string` |
    """
)

# The walkthrough's OLD inline pattern (demo/walkthrough.sh:261 before
# str-qwua7.10): confirms the exact reported gap — this pattern does not
# match the real runtime message, "input N deserialization failed", only the
# differently-worded "failed to deserialize".
LEGACY_WALKTHROUGH_ERROR_RE = re.compile(
    r"\[error\]|failed to deserialize|panic|SIGSEGV|error: exploration error",
    re.IGNORECASE,
)

# Captured shape of a scan summary-table row (shatter-core/src/report.rs
# write_md_summary_table) at 0% coverage with a non-FAIL status label — since
# str-4ad5 (2026-05-22), coverage-based failures are labelled LOW/WARN/PASS,
# never FAIL, so the old FAIL_ROW_RE alone can't see this.
SCAN_ROW_ZERO_COVERAGE = (
    "| LOW | error_only | `brandNewZeroFn` | /tmp/x/standalone/ts/99-novel.ts | 0.0% | 0/4 | 0/12 | 5 |\n"
)

# The real thrown message from examples/standalone/ts/setup-file-level.ts's
# `teardown`, as rendered by a behavior-cluster line
# (shatter-core/src/report.rs write_md_function_details: "throws {err}").
LIFECYCLE_CLUSTER_EXCERPT = (
    "### `teardown`\n\n"
    "**Behaviors:**\n\n"
    "- Cluster 1: throws Error: Teardown scope mismatch: expected a, got b (inputs: [\"a\", {}])\n"
)


class GauntletCheckOutputZeroCoverageTest(unittest.TestCase):
    def test_rust_deserialization_zero_coverage_flagged(self) -> None:
        result = run_helper(RUST_ZERO_COVERAGE_EXCERPT)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("deserialization failed", result.stdout)

    def test_legacy_walkthrough_pattern_missed_this_excerpt(self) -> None:
        # Proves the pre-str-qwua7.10 walkthrough regex silently marked this
        # exact regression "(ok)".
        self.assertIsNone(LEGACY_WALKTHROUGH_ERROR_RE.search(RUST_ZERO_COVERAGE_EXCERPT))

    def test_zero_coverage_without_error_keyword_flagged(self) -> None:
        # 0% coverage with no "error"/"panic"/"deserializ*" keyword anywhere
        # (e.g. every call returns some placeholder), isolating the
        # heading+summary detection path from PROCESS_ERROR_RE.
        excerpt = textwrap.dedent(
            """\
            ## `alwaysNull` *(/tmp/x/standalone/ts/99-novel.ts:1-9)*

            **2 path(s)** · **0%** coverage (0/9 lines)

            | # | Call | Outcome |
            |---|---|---|
            | 1 | `alwaysNull(1)` | returns `null` |
            | 2 | `alwaysNull(2)` | returns `null` |
            """
        )
        result = run_helper(excerpt)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("alwaysNull", result.stdout)
        self.assertIn("0% coverage", result.stdout)

    def test_zero_coverage_allowlisted_function_passes(self) -> None:
        with TemporaryDirectory() as td:
            allowlist = write_allowlist(
                td,
                textwrap.dedent(
                    """\
                    expected_failures:
                      - file: 99-novel.ts
                        function: alwaysNull
                        reason: intentionally unsupported fixture
                        expires: "2099-01-01"
                    """
                ),
            )
            excerpt = textwrap.dedent(
                """\
                ## `alwaysNull` *(/tmp/x/standalone/ts/99-novel.ts:1-9)*

                **2 path(s)** · **0%** coverage (0/9 lines)

                | # | Call | Outcome |
                |---|---|---|
                | 1 | `alwaysNull(1)` | returns `null` |
                """
            )
            result = run_helper(excerpt, allowlist=allowlist)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_scan_row_zero_coverage_non_fail_status_flagged(self) -> None:
        # The pre-str-qwua7.10 checker only looked for literal "| FAIL |"
        # rows, which the live report format (report.rs write_md_summary_table,
        # PASS/WARN/LOW since str-4ad5) never emits for coverage failures.
        result = run_helper(SCAN_ROW_ZERO_COVERAGE)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("brandNewZeroFn", result.stdout)

    def test_nonzero_low_coverage_row_not_flagged_by_zero_coverage_check(self) -> None:
        # A genuinely low (but nonzero) coverage row must not trip the new
        # check — only exact 0% with >=1 iteration should.
        row = "| WARN | completed | `partiallyCovered` | /tmp/x/standalone/ts/99-novel.ts | 19.0% | 2/13 | 19/102 | 100 |\n"
        result = run_helper(row)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


class GauntletCheckOutputLifecycleClusterTest(unittest.TestCase):
    def test_teardown_scope_mismatch_cluster_flagged(self) -> None:
        result = run_helper(LIFECYCLE_CLUSTER_EXCERPT)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Teardown scope mismatch", result.stdout)

    def test_teardown_scope_mismatch_allowlisted_passes(self) -> None:
        with TemporaryDirectory() as td:
            allowlist = write_allowlist(
                td,
                textwrap.dedent(
                    """\
                    expected_failures: []
                    expected_lifecycle_clusters:
                      - class: "Teardown scope mismatch"
                        reason: pending discovery-side fix, str-qwua7.56
                        expires: "2099-01-01"
                    """
                ),
            )
            result = run_helper(LIFECYCLE_CLUSTER_EXCERPT, allowlist=allowlist)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


# Byte-accurate excerpt of the walkthrough's real Step 7 ("Explore Rust
# Functions") output, captured 2026-09-24 by running `task affected` on this
# branch against a *fresh* examples checkout (scripts/examples_checkout.py
# --fresh, per demo/walkthrough.sh) — i.e. this is not a hypothetical, it is
# what the gate actually printed that run. classify_number hit an unrelated
# "index out of bounds" harness panic (also 0% coverage, no
# "deserialization failed" text — proving the heading+summary check, not
# just the PROCESS_ERROR_RE addition, is needed); classify_string and
# negotiate_language hit the deserialization mismatch; safe_divide happened
# to pass this run (str-qwua7.14's underlying bug is flaky against the
# walkthrough's 3-iteration budget — it also failed the same way earlier
# this session, just not in this particular capture). Tab/path prefixes
# trimmed of their /tmp/shatter-walkthrough-artifacts.* noise; the
# `[info] Wrote explore artifact ...` and `[batch ...]` lines are left out
# since they carry no signal for this checker.
LIVE_STEP7_EXCERPT = textwrap.dedent(
    """\
    ## `classify_number` *(/tmp/shatter-examples.6cknrlex/standalone/rust/01_arithmetic.rs:6-18)*

    **1 path(s)** · **0%** coverage (0/13 lines)

    | # | Call | Outcome |
    |---|---|---|
    | 1 | `classify_number(0)` | throws `runtime_error: index out of bounds: the len is 1 but the index is 1` |

    ## `classify_string` *(/tmp/shatter-examples.6cknrlex/standalone/rust/02_strings.rs:7-24)*

    **3 path(s)** · **0%** coverage (0/18 lines)

    | # | Call | Outcome |
    |---|---|---|
    | 1 | `classify_string("empty")` | throws `runtime_error: input 0 deserialization failed: invalid type: string "empty", expected f64` |
    | 2 | `classify_string("single-char")` | throws `runtime_error: input 0 deserialization failed: invalid type: string "single-char", expected f64` |
    | 3 | `classify_string("http")` | throws `runtime_error: input 0 deserialization failed: invalid type: string "http", expected f64` |
    - *Mocks: all, is_ascii_digit*

    ## `safe_divide` *(/tmp/shatter-examples.6cknrlex/standalone/rust/04_errors.rs:6-14)*

    **2 path(s)** · **44%** coverage (4/9 lines)

    | # | Call | Outcome |
    |---|---|---|
    | 1 | `safe_divide(0.0, 0.0)` | returns `{"Err":"division by zero"}` |
    | 2 | `safe_divide(0.0, -1.0)` | returns `{"Ok":-0.0}` |
    - *Mocks: to_string*

    ## `negotiate_language` *(/tmp/shatter-examples.6cknrlex/standalone/rust/18_accept_language.rs:101-202)*

    **3 path(s)** · **0%** coverage (0/102 lines)

    | # | Call | Outcome |
    |---|---|---|
    | 1 | `negotiate_language("*", [])` | throws `runtime_error: input 0 deserialization failed: invalid type: string "*", expected f64` |
    | 2 | `negotiate_language("", ["*"])` | throws `runtime_error: input 0 deserialization failed: invalid type: string "", expected f64` |
    | 3 | `negotiate_language("-", [])` | throws `runtime_error: input 0 deserialization failed: invalid type: string "-", expected f64` |
    - *Mocks: enumerate, filter_map, is_empty, total_cmp, cmp, sort_by, map, iter, collect, then, starts_with, to_ascii_lowercase, clone*
    """
)


class GauntletCheckOutputLiveRegressionTest(unittest.TestCase):
    """str-qwua7.10 acceptance check: with the two str-qwua7.14 allowlist
    entries in place, the live-captured Step 7 regression passes; remove
    them and it goes red again."""

    def test_live_excerpt_passes_with_current_allowlist(self) -> None:
        result = run_helper(LIVE_STEP7_EXCERPT)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_crash_marker_after_allowlisted_block_is_still_flagged(self) -> None:
        # Review regression: a single-function explore never emits a closing
        # "---", so the allowlisted function's block stays "open" and used to
        # swallow a real crash line (interleaved stderr) that followed it.
        text = LIVE_STEP7_EXCERPT + "\nthread 'main' panicked: boom\n[error] frontend died\nSIGSEGV\n"
        result = run_helper(text)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("panicked", result.stdout)
        self.assertIn("[error] frontend died", result.stdout)
        self.assertIn("SIGSEGV", result.stdout)
        # ...while the allowlisted function's own deserialization rows stay
        # excused.
        self.assertNotIn("deserialization failed", result.stdout)

    def test_interleaved_log_line_does_not_hide_a_zero_percent_summary(self) -> None:
        # Review regression: stdout+stderr are tee'd into one capture, so an
        # "[info]" log line can land between a heading and its summary line;
        # it used to consume the one-shot summary check.
        text = textwrap.dedent(
            """\
            ## `brand_new_fn` *(/tmp/x/standalone/rust/99_new.rs:1-9)*
            [info] Wrote explore artifact /tmp/x/artifact.json

            **2 path(s)** · **0%** coverage (0/9 lines)
            """
        )
        result = run_helper(text)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("brand_new_fn: 0% coverage", result.stdout)

    def test_live_excerpt_fails_without_the_rust_entries(self) -> None:
        with TemporaryDirectory() as td:
            allowlist = write_allowlist(
                td,
                textwrap.dedent(
                    """\
                    expected_failures: []
                    """
                ),
            )
            result = run_helper(LIVE_STEP7_EXCERPT, allowlist=allowlist)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("classify_number", result.stdout)
            self.assertIn("classify_string", result.stdout)
            self.assertIn("negotiate_language", result.stdout)
            # safe_divide passed this run (44% coverage) — must never be
            # flagged regardless of allowlist contents.
            self.assertNotIn("safe_divide", result.stdout)


class GauntletCheckOutputAllowlistExpiryTest(unittest.TestCase):
    def test_expired_expected_failures_entry_flagged(self) -> None:
        with TemporaryDirectory() as td:
            allowlist = write_allowlist(
                td,
                textwrap.dedent(
                    """\
                    expected_failures:
                      - file: loop.ts
                        function: spin
                        reason: stale entry
                        expires: "2000-01-01"
                    """
                ),
            )
            result = run_scan_json(FAILED_JSON, allowlist.read_text())
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("ALLOWLIST ENTRY EXPIRED", result.stdout)
            # And, since the entry is no longer active, the failure it used to
            # suppress is flagged too.
            self.assertIn("spin", result.stdout)

    def test_missing_expires_is_a_hard_allowlist_error(self) -> None:
        with TemporaryDirectory() as td:
            allowlist = write_allowlist(
                td,
                textwrap.dedent(
                    """\
                    expected_failures:
                      - file: 04-errors.ts
                        function: computeStats
                        reason: missing expiry
                    """
                ),
            )
            result = run_helper("clean output\n", allowlist=allowlist)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("expires", result.stdout)

    def test_live_repo_allowlist_has_no_expired_entries(self) -> None:
        # Guards against the real allowlist silently rotting: a clean run
        # against the live demo/gauntlet-scan-allowlist.yaml must not report
        # any expired entries today.
        result = run_helper("clean output\n")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("ALLOWLIST ENTRY EXPIRED", result.stdout)


CHECK_STEP_LIB = REPO_ROOT / "demo" / "check_step_lib.sh"


def scan_step_setup(*shatter_args: str) -> tuple[list[str], list[str]]:
    """Source demo/check_step_lib.sh, run scan_step_setup, and return
    (SCAN_JSON_EXTRA_ARGS, SCAN_CHECK_ARGS)."""
    script = (
        'source "$1"; shift; scan_step_setup /host /cmd "$@"; '
        'printf "%s\\n" ${SCAN_JSON_EXTRA_ARGS[@]+"${SCAN_JSON_EXTRA_ARGS[@]}"}; '
        "echo '---'; "
        'printf "%s\\n" ${SCAN_CHECK_ARGS[@]+"${SCAN_CHECK_ARGS[@]}"}'
    )
    result = subprocess.run(
        ["bash", "-c", script, "bash", str(CHECK_STEP_LIB), *shatter_args],
        capture_output=True,
        text=True,
        check=True,
    )
    extra, _, check_args = result.stdout.partition("---\n")
    # printf '%s\n' with an empty array still prints one blank line.
    return [a for a in extra.splitlines() if a], [a for a in check_args.splitlines() if a]


class CheckStepLibScanSetupTest(unittest.TestCase):
    def test_scan_step_writes_json_and_keeps_the_markdown_report_on_stdout(self) -> None:
        # `scan -o FILE` alone silences the per-function markdown report on
        # stdout (4.5 KB -> 0.5 KB on one file), which would blind the 0%-coverage
        # table check and empty the demo output. `--stdout` keeps both.
        extra, check_args = scan_step_setup("scan", "examples/standalone/ts")
        self.assertEqual(extra[0], "-o")
        self.assertTrue(extra[1].startswith("/cmd/shatter-scan-json."), extra)
        self.assertIn("--stdout", extra)
        self.assertEqual(check_args[0], "--scan-json")

    def test_non_scan_step_gets_no_extra_args(self) -> None:
        extra, check_args = scan_step_setup("explore", "x.ts:f")
        self.assertEqual(extra, [])
        self.assertEqual(check_args, [])

    def test_dry_run_scan_skips_the_json_check(self) -> None:
        extra, check_args = scan_step_setup("scan", "--dry-run", "examples/standalone/ts")
        self.assertEqual(extra, [])
        self.assertEqual(check_args[0], "--no-scan-json")

    def test_timeout_total_scan_expects_interruption(self) -> None:
        _, check_args = scan_step_setup("scan", "--timeout-total", "120", "examples/standalone/ts")
        self.assertIn("--expect-interrupted", check_args)


if __name__ == "__main__":
    unittest.main()
