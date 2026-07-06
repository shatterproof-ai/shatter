"""Tests for scripts/docs-smoke.py.

These tests exercise the pure logic (block extraction, directive parsing,
shell-invocation validation, JSON/YAML validation) with a synthetic CliSpec so
they do not require a built `shatter` binary.
"""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("docs-smoke.py")
SPEC = importlib.util.spec_from_file_location("docs_smoke", MODULE_PATH)
assert SPEC is not None
assert SPEC.loader is not None
docs_smoke = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = docs_smoke
SPEC.loader.exec_module(docs_smoke)


def make_spec() -> "docs_smoke.CliSpec":
    """A synthetic CLI spec mirroring the real command surface closely enough
    to test flag/subcommand validation deterministically."""
    spec = docs_smoke.CliSpec()
    spec.command_paths = {
        ("explore",),
        ("scan",),
        ("list-targets",),
        ("diff",),
        ("cache",),
        ("cache", "clear"),
    }
    spec.long_flags = {
        ("explore",): {"--concolic", "--spec", "--spec-json", "--spec-out",
                       "--timeout-explore", "--per-function-timeout", "--time-limit"},
        ("scan",): {"--include", "--exclude", "--changed", "--language",
                    "--resume", "--progress"},
        ("list-targets",): {"--language", "--include", "--exclude", "--format", "--output"},
        ("diff",): {"--json"},
        ("cache",): set(),
        ("cache", "clear"): set(),
    }
    spec.short_flags = {
        ("explore",): {"-o"},
        ("scan",): {"-o"},
        ("list-targets",): {"-o"},
        ("diff",): set(),
        ("cache",): set(),
        ("cache", "clear"): set(),
    }
    spec.global_long = {"--log-level", "--verbose", "--quiet", "--color", "--set"}
    spec.global_short = {"-v", "-q"}
    return spec


# ---------------------------------------------------------------------------
# Shell invocation validation — the core stale-flag path
# ---------------------------------------------------------------------------


class ShatterInvocationTest(unittest.TestCase):
    def setUp(self) -> None:
        self.spec = make_spec()

    def _check(self, cmd: str) -> list[str]:
        import shlex
        return docs_smoke.check_shatter_invocation(shlex.split(cmd), self.spec)

    def test_valid_explore_flags_pass(self) -> None:
        self.assertEqual(self._check("shatter explore --concolic --spec-json foo.ts:bar"), [])

    def test_valid_scan_flags_pass(self) -> None:
        self.assertEqual(
            self._check("shatter scan --include '**/*.ts' --exclude '**/vendor/**' src/"), [])

    def test_stale_explore_timeout_flag_fails(self) -> None:
        # The canonical regression: `shatter explore --timeout` was removed
        # (the real flag is --timeout-explore). Reintroducing it must fail.
        errors = self._check("shatter explore --timeout 30 foo.ts:bar")
        self.assertTrue(errors)
        self.assertTrue(any("--timeout" in e and "unknown flag" in e for e in errors))

    def test_stale_scan_output_dir_flag_fails(self) -> None:
        errors = self._check("shatter scan --output-dir out/ src/")
        self.assertTrue(any("--output-dir" in e for e in errors))

    def test_stale_global_perf_flag_fails(self) -> None:
        errors = self._check("shatter explore --perf foo.ts:bar")
        self.assertTrue(any("--perf" in e for e in errors))

    def test_removed_subcommand_fails(self) -> None:
        errors = self._check("shatter frobnicate foo.ts")
        self.assertTrue(any("unknown subcommand" in e and "frobnicate" in e for e in errors))

    def test_global_flag_allowed_on_subcommand(self) -> None:
        self.assertEqual(self._check("shatter --log-level debug scan src/"), [])

    def test_short_flag_valid(self) -> None:
        self.assertEqual(self._check("shatter list-targets --format json -o targets.json ."), [])

    def test_unknown_short_flag_fails(self) -> None:
        errors = self._check("shatter list-targets -z .")
        self.assertTrue(any("-z" in e for e in errors))

    def test_nested_subcommand_resolves(self) -> None:
        # `cache clear` resolves to the two-token path; no flags => no error.
        self.assertEqual(self._check("shatter cache clear"), [])

    def test_positional_not_treated_as_subcommand(self) -> None:
        # `src/` after scan is a positional, not an unknown subcommand.
        self.assertEqual(self._check("shatter scan src/"), [])

    def test_version_and_help_always_allowed(self) -> None:
        self.assertEqual(self._check("shatter --version"), [])
        self.assertEqual(self._check("shatter --help"), [])
        self.assertEqual(self._check("shatter explore --help foo.ts"), [])

    def test_double_dash_ends_option_parsing(self) -> None:
        self.assertEqual(self._check("shatter explore -- --not-a-flag"), [])


# ---------------------------------------------------------------------------
# Shell command extraction
# ---------------------------------------------------------------------------


class ShellExtractionTest(unittest.TestCase):
    def test_extracts_only_shatter_lines(self) -> None:
        content = (
            "# a comment\n"
            "cargo build --release\n"
            "shatter scan src/\n"
            "curl -sSL https://example.com | bash\n"
        )
        cmds = docs_smoke.parse_shell_commands(content)
        self.assertEqual(cmds, [["shatter", "scan", "src/"]])

    def test_strips_prompt_and_ignores_output(self) -> None:
        content = (
            "$ shatter scan --progress src/\n"
            '{"type":"progress","status":"started"}\n'
            "Scan complete: 3 completed\n"
            "^C  # interrupted\n"
        )
        cmds = docs_smoke.parse_shell_commands(content)
        self.assertEqual(cmds, [["shatter", "scan", "--progress", "src/"]])

    def test_handles_release_binary_path(self) -> None:
        cmds = docs_smoke.parse_shell_commands("./target/release/shatter explore foo.ts:bar\n")
        self.assertEqual(cmds, [["./target/release/shatter", "explore", "foo.ts:bar"]])


# ---------------------------------------------------------------------------
# JSON / YAML validation
# ---------------------------------------------------------------------------


class StructuredValidationTest(unittest.TestCase):
    def test_valid_json_passes(self) -> None:
        self.assertIsNone(docs_smoke.validate_json_block('{"a": 1, "b": [2, 3]}'))

    def test_invalid_json_fails(self) -> None:
        # Trailing comma is invalid JSON.
        err = docs_smoke.validate_json_block('{"a": 1,}')
        self.assertIsNotNone(err)
        self.assertIn("invalid JSON", err)

    def test_ndjson_stream_fails_as_single_json(self) -> None:
        # Multiple objects, one per line — not a single JSON document.
        err = docs_smoke.validate_json_block('{"a":1}\n{"a":2}')
        self.assertIsNotNone(err)

    def test_valid_yaml_passes(self) -> None:
        self.assertIsNone(docs_smoke.validate_yaml_block("defaults:\n  max_iterations: 50\n"))

    def test_invalid_yaml_fails(self) -> None:
        err = docs_smoke.validate_yaml_block("a:\n  b: 1\n bad: : :\n")
        self.assertIsNotNone(err)


# ---------------------------------------------------------------------------
# Directive parsing (no-run exemptions)
# ---------------------------------------------------------------------------


class DirectiveTest(unittest.TestCase):
    def test_no_directive(self) -> None:
        self.assertEqual(docs_smoke.extract_directive("some prose"), (False, None, None))
        self.assertEqual(docs_smoke.extract_directive(None), (False, None, None))

    def test_skip_with_reason(self) -> None:
        skip, reason, err = docs_smoke.extract_directive(
            '<!-- docs-smoke: skip reason="NDJSON stream" -->')
        self.assertTrue(skip)
        self.assertEqual(reason, "NDJSON stream")
        self.assertIsNone(err)

    def test_skip_without_reason_is_error(self) -> None:
        skip, reason, err = docs_smoke.extract_directive("<!-- docs-smoke: skip -->")
        self.assertTrue(skip)
        self.assertIsNone(reason)
        self.assertIsNotNone(err)
        self.assertIn("requires a non-empty reason", err)

    def test_skip_with_empty_reason_is_error(self) -> None:
        _, _, err = docs_smoke.extract_directive('<!-- docs-smoke: skip reason="" -->')
        self.assertIsNotNone(err)

    def test_unknown_directive_is_error(self) -> None:
        _, _, err = docs_smoke.extract_directive("<!-- docs-smoke: yolo -->")
        self.assertIsNotNone(err)
        self.assertIn("unknown docs-smoke directive", err)


# ---------------------------------------------------------------------------
# Fenced block extraction + directive attachment
# ---------------------------------------------------------------------------


class FencedBlockTest(unittest.TestCase):
    def test_lang_and_content(self) -> None:
        text = "intro\n\n```json\n{\"a\":1}\n```\n"
        blocks = docs_smoke.parse_fenced_blocks(text)
        self.assertEqual(len(blocks), 1)
        self.assertEqual(blocks[0].lang, "json")
        self.assertEqual(blocks[0].content, '{"a":1}')

    def test_skip_directive_attaches_to_next_block(self) -> None:
        text = (
            '<!-- docs-smoke: skip reason="illustrative" -->\n'
            "```json\n"
            "{not json at all}\n"
            "```\n"
        )
        blocks = docs_smoke.parse_fenced_blocks(text)
        self.assertTrue(blocks[0].skip)
        self.assertEqual(blocks[0].skip_reason, "illustrative")

    def test_directive_through_blank_line(self) -> None:
        text = (
            '<!-- docs-smoke: skip reason="x" -->\n'
            "\n"
            "```json\n"
            "nope\n"
            "```\n"
        )
        blocks = docs_smoke.parse_fenced_blocks(text)
        self.assertTrue(blocks[0].skip)

    def test_escaped_inner_fence_not_treated_as_close(self) -> None:
        # A backslash-escaped ``` inside a block is content, not a fence.
        text = "```markdown\n\\```\ninner\n\\```\n```\n"
        blocks = docs_smoke.parse_fenced_blocks(text)
        self.assertEqual(len(blocks), 1)
        self.assertEqual(blocks[0].lang, "markdown")
        self.assertIn("inner", blocks[0].content)


# ---------------------------------------------------------------------------
# End-to-end doc validation (in-memory doc + synthetic spec)
# ---------------------------------------------------------------------------


class ValidateDocTest(unittest.TestCase):
    def setUp(self) -> None:
        self.spec = make_spec()

    def _validate(self, text: str) -> "docs_smoke.Result":
        result = docs_smoke.Result()
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "DOC.md"
            p.write_text(text)
            docs_smoke.validate_doc(p, "DOC.md", self.spec, result, verbose=False)
        return result

    def test_clean_doc_passes(self) -> None:
        text = (
            "# Doc\n\n"
            "```bash\nshatter explore --concolic foo.ts:bar\n```\n\n"
            "```json\n{\"include\": [\"src\"]}\n```\n"
        )
        result = self._validate(text)
        self.assertTrue(result.ok(), msg=f"unexpected: {result.errors}")

    def test_stale_flag_in_doc_fails(self) -> None:
        text = "# Doc\n\n```bash\nshatter explore --timeout 30 foo.ts:bar\n```\n"
        result = self._validate(text)
        self.assertFalse(result.ok())
        self.assertTrue(any("--timeout" in e for e in result.errors))

    def test_invalid_json_in_doc_fails(self) -> None:
        text = "# Doc\n\n```json\n{\"a\": 1,}\n```\n"
        result = self._validate(text)
        self.assertFalse(result.ok())
        self.assertTrue(any("invalid JSON" in e for e in result.errors))

    def test_skip_directive_exempts_bad_block(self) -> None:
        text = (
            "# Doc\n\n"
            '<!-- docs-smoke: skip reason="NDJSON stream" -->\n'
            "```json\n{\"a\":1}\n{\"a\":2}\n```\n"
        )
        result = self._validate(text)
        self.assertTrue(result.ok(), msg=f"unexpected: {result.errors}")

    def test_bare_skip_directive_fails(self) -> None:
        text = "# Doc\n\n<!-- docs-smoke: skip -->\n```json\nnot json\n```\n"
        result = self._validate(text)
        self.assertFalse(result.ok())
        self.assertTrue(any("requires a non-empty reason" in e for e in result.errors))


# ---------------------------------------------------------------------------
# Help-output parsing (spec builder helpers)
# ---------------------------------------------------------------------------


class HelpParsingTest(unittest.TestCase):
    SAMPLE_HELP = (
        "Explore functions.\n\n"
        "Usage: shatter explore [OPTIONS] <TARGETS>...\n\n"
        "Arguments:\n"
        "  <TARGETS>...  Targets to explore\n\n"
        "Options:\n"
        "      --concolic            Use the concolic explorer\n"
        "  -o, --output <PATH>       Write report to file\n"
        "      --timeout-explore <SECONDS>  Per-function wall-clock timeout\n"
        "  -h, --help                Print help\n"
    )

    def test_parse_flags(self) -> None:
        longs, shorts = docs_smoke._parse_help_flags(self.SAMPLE_HELP)
        self.assertIn("--concolic", longs)
        self.assertIn("--output", longs)
        self.assertIn("--timeout-explore", longs)
        self.assertIn("-o", shorts)
        # Must NOT invent --timeout from --timeout-explore.
        self.assertNotIn("--timeout", longs)

    def test_parse_subcommands(self) -> None:
        root = (
            "Usage: shatter [OPTIONS] <COMMAND>\n\n"
            "Commands:\n"
            "  explore       Explore functions\n"
            "  scan          Scan a directory\n"
            "  list-targets  List targets\n"
            "  help          Print this message\n\n"
            "Options:\n"
            "  -h, --help  Print help\n"
        )
        subs = docs_smoke._parse_help_subcommands(root)
        self.assertIn("explore", subs)
        self.assertIn("scan", subs)
        self.assertIn("list-targets", subs)
        self.assertNotIn("help", subs)


if __name__ == "__main__":
    unittest.main()
