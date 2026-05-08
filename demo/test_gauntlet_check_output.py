"""Tests for demo/gauntlet_check_output.py (str-jeen.59)."""

import subprocess
import sys
import textwrap
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

REPO_ROOT = Path(__file__).resolve().parent.parent
SCRIPT = REPO_ROOT / "demo" / "gauntlet_check_output.py"
ALLOWLIST = REPO_ROOT / "demo" / "gauntlet-scan-allowlist.yaml"


def run_helper(output_text: str, allowlist: Path = ALLOWLIST) -> subprocess.CompletedProcess:
    with TemporaryDirectory() as td:
        out = Path(td) / "out.txt"
        out.write_text(output_text)
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--allowlist", str(allowlist), "--output", str(out), "--step", "test"],
            capture_output=True,
            text=True,
            check=False,
        )


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


if __name__ == "__main__":
    unittest.main()
