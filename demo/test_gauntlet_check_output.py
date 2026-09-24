"""Tests for demo/gauntlet_check_output.py (str-jeen.59; str-qwua7.10 extends
coverage to 0%-coverage function blocks, lifecycle/scope-mismatch thrown-error
clusters, and allowlist expiry)."""

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


# A canonical scan-output snippet matching the current gauntlet baseline:
# 15 allowlisted FAIL rows + a "2 error(s)" summary that matches expected_scan_errors.count.
BASELINE_OUTPUT = textwrap.dedent(
    """\
    Scan complete: **43 function(s)** tested, **0 skipped**, **2 error(s)** (16 worker(s))
    | FAIL | error_only | `computeStats` | /tmp/x/standalone/ts/04-errors.ts | 25.0% | 1/5 | 4/16 | 100 |
    | FAIL | behavioral | `computeArea` | /tmp/x/standalone/ts/05-unions.ts | 10.0% | 0/6 | 1/10 | 100 |
    | FAIL | behavioral | `routeRequest` | /tmp/x/standalone/ts/05-unions.ts | 23.1% | 1/8 | 3/13 | 100 |
    | FAIL | error_only | `processStateMachine` | /tmp/x/standalone/ts/06-nested-control-flow.ts | 13.8% | 1/12 | 4/29 | 100 |
    | FAIL | behavioral | `authorizeRequest` | /tmp/x/standalone/ts/07-auth-validation.ts | 24.1% | 4/14 | 7/29 | 100 |
    | FAIL | behavioral | `validateJwt` | /tmp/x/standalone/ts/07-auth-validation.ts | 19.2% | 2/8 | 5/26 | 100 |
    | FAIL | behavioral | `matchRoute` | /tmp/x/standalone/ts/10-path-router.ts | 24.0% | 4/19 | 12/50 | 100 |
    | FAIL | error_only | `parseSemver` | /tmp/x/standalone/ts/14-semver.ts | 36.4% | 3/6 | 8/22 | 100 |
    | FAIL | error_only | `classifyConfigs` | /tmp/x/standalone/ts/17-mock-branches.ts | 25.0% | 1/4 | 3/12 | 100 |
    | FAIL | error_only | `classifyStatus` | /tmp/x/standalone/ts/17-mock-branches.ts | 12.5% | 0/3 | 1/8 | 100 |
    | FAIL | behavioral | `loadOrDefault` | /tmp/x/standalone/ts/17-mock-branches.ts | 33.3% | 1/2 | 2/6 | 100 |
    | FAIL | error_only | `negotiateLanguage` | /tmp/x/standalone/ts/18-accept-language.ts | 7.1% | 1/12 | 2/28 | 100 |
    | FAIL | behavioral | `evaluateRobotsPolicy` | /tmp/x/standalone/ts/19-robots-policy.ts | 39.3% | 3/9 | 11/28 | 100 |
    | FAIL | behavioral | `parseDotenv` | /tmp/x/standalone/ts/20-dotenv-parser.ts | 28.9% | 4/12 | 13/45 | 100 |
    | FAIL | error_only | `classifySecret` | /tmp/x/standalone/ts/21-crypto-boundary.ts | 28.6% | 0/2 | 2/7 | 100 |
    """
)


class GauntletCheckOutputTest(unittest.TestCase):
    def test_baseline_passes(self) -> None:
        result = run_helper(BASELINE_OUTPUT)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout, "")

    def test_unallowlisted_fail_row_flagged(self) -> None:
        novel = "| FAIL | behavioral | `brandNewFn` | /tmp/x/standalone/ts/99-novel.ts | 0.0% | 0/1 | 0/5 | 100 |\n"
        result = run_helper(BASELINE_OUTPUT + novel)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("brandNewFn", result.stdout)

    def test_excess_scan_errors_flagged(self) -> None:
        bumped = BASELINE_OUTPUT.replace("2 error(s)", "3 error(s)", 1)
        result = run_helper(bumped)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("error(s)", result.stdout)

    def test_zero_scan_errors_passes(self) -> None:
        clean = "Scan complete: **1 function(s)** tested, **0 skipped**, **0 error(s)** (1 worker(s))\n"
        result = run_helper(clean)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_process_level_error_flagged(self) -> None:
        result = run_helper("[error] frontend crashed\nthread 'main' panicked at 'oops'\n")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("[error]", result.stdout)

    def test_allowlisted_fail_with_different_path_prefix(self) -> None:
        # Allowlist matches by basename, not full path, so different tmpdir prefixes still match.
        with_prefix = "| FAIL | error_only | `computeStats` | /var/folders/abc/standalone/ts/04-errors.ts | 25.0% | 1/5 | 4/16 | 100 |\n"
        result = run_helper(with_prefix + "Scan complete: **1 function(s)** tested, **0 skipped**, **0 error(s)** (1 worker(s))\n")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


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
                        outcome: FAIL
                        category: error_only
                        reason: intentionally unsupported fixture
                        expires: "2099-01-01"
                    expected_scan_errors:
                      count: 0
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
                    expected_scan_errors:
                      count: 0
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
                    expected_scan_errors:
                      count: 0
                      expires: "2099-01-01"
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
                      - file: 04-errors.ts
                        function: computeStats
                        outcome: FAIL
                        category: error_only
                        reason: stale entry
                        expires: "2000-01-01"
                    expected_scan_errors:
                      count: 0
                      expires: "2099-01-01"
                    """
                ),
            )
            row = "| FAIL | error_only | `computeStats` | /tmp/x/standalone/ts/04-errors.ts | 25.0% | 1/5 | 4/16 | 100 |\n"
            result = run_helper(row, allowlist=allowlist)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("ALLOWLIST ENTRY EXPIRED", result.stdout)
            # And, since the entry is no longer active, the row it used to
            # suppress is flagged too.
            self.assertIn("computeStats", result.stdout)

    def test_missing_expires_is_a_hard_allowlist_error(self) -> None:
        with TemporaryDirectory() as td:
            allowlist = write_allowlist(
                td,
                textwrap.dedent(
                    """\
                    expected_failures:
                      - file: 04-errors.ts
                        function: computeStats
                        outcome: FAIL
                        category: error_only
                        reason: missing expiry
                    expected_scan_errors:
                      count: 0
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


if __name__ == "__main__":
    unittest.main()
