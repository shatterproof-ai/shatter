"""Tests for scripts/validate-protocol-registry.py — IDL field-model layer."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("validate-protocol-registry.py")
SPEC = importlib.util.spec_from_file_location("validate_protocol_registry", MODULE_PATH)
assert SPEC is not None
assert SPEC.loader is not None
validate_protocol_registry = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = validate_protocol_registry
SPEC.loader.exec_module(validate_protocol_registry)


REPO_ROOT = MODULE_PATH.resolve().parent.parent
LIVE_REGISTRY = REPO_ROOT / "protocol" / "registry.yaml"


def _write_registry(tmp_dir: str, body: str) -> Path:
    path = Path(tmp_dir) / "registry.yaml"
    path.write_text(body)
    return path


_MINIMAL_VALID = """
enums:
  setup_level:
    description: Lifecycle scope.
    values: [session, file]
commands:
  shutdown:
    description: Shutdown.
    fields: []
    response_status: shutdown_ack
    field_model:
      request_fields: {}
      response_fields: {}
  teardown:
    description: Teardown.
    fields: [scope, level]
    response_status: teardown_ack
    field_model:
      request_fields:
        scope: { type: string, optional: false }
        level: { type: "enum:setup_level", optional: false }
      response_fields: {}
"""


class FieldModelLayerTest(unittest.TestCase):
    def test_minimal_registry_passes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = _write_registry(tmp, _MINIMAL_VALID)
            errors = validate_protocol_registry.validate_field_model(path)
            self.assertEqual(errors, [], msg=errors)

    def test_live_registry_passes(self) -> None:
        """The checked-in registry must satisfy the IDL layer."""
        errors = validate_protocol_registry.validate_field_model(LIVE_REGISTRY)
        self.assertEqual(errors, [], msg=errors)

    def test_missing_field_model_fails(self) -> None:
        body = """
enums: {}
commands:
  shutdown:
    description: Shutdown.
    fields: []
    response_status: shutdown_ack
"""
        with tempfile.TemporaryDirectory() as tmp:
            path = _write_registry(tmp, body)
            errors = validate_protocol_registry.validate_field_model(path)
            self.assertTrue(
                any("missing required `field_model:`" in e for e in errors),
                msg=errors,
            )

    def test_undefined_enum_reference_fails(self) -> None:
        body = """
enums: {}
commands:
  teardown:
    description: Teardown.
    fields: [level]
    response_status: teardown_ack
    field_model:
      request_fields:
        level: { type: "enum:setup_level", optional: false }
      response_fields: {}
"""
        with tempfile.TemporaryDirectory() as tmp:
            path = _write_registry(tmp, body)
            errors = validate_protocol_registry.validate_field_model(path)
            self.assertTrue(
                any("references undefined enum" in e for e in errors),
                msg=errors,
            )

    def test_flat_fields_superset_mismatch_fails(self) -> None:
        # `fields:` mentions a name not present in field_model.request_fields.
        body = """
enums: {}
commands:
  teardown:
    description: Teardown.
    fields: [scope, level, bogus]
    response_status: teardown_ack
    field_model:
      request_fields:
        scope: { type: string, optional: false }
        level: { type: string, optional: false }
      response_fields: {}
"""
        with tempfile.TemporaryDirectory() as tmp:
            path = _write_registry(tmp, body)
            errors = validate_protocol_registry.validate_field_model(path)
            self.assertTrue(
                any("'bogus' is not declared" in e for e in errors),
                msg=errors,
            )

    def test_field_model_extras_missing_from_flat_fields_fails(self) -> None:
        # field_model declares a field that `fields:` omits.
        body = """
enums: {}
commands:
  teardown:
    description: Teardown.
    fields: [scope]
    response_status: teardown_ack
    field_model:
      request_fields:
        scope: { type: string, optional: false }
        level: { type: string, optional: false }
      response_fields: {}
"""
        with tempfile.TemporaryDirectory() as tmp:
            path = _write_registry(tmp, body)
            errors = validate_protocol_registry.validate_field_model(path)
            self.assertTrue(
                any("missing 'level'" in e for e in errors),
                msg=errors,
            )

    def test_unrecognized_type_fails(self) -> None:
        body = """
enums: {}
commands:
  teardown:
    description: Teardown.
    fields: [scope]
    response_status: teardown_ack
    field_model:
      request_fields:
        scope: { type: weird_type, optional: false }
      response_fields: {}
"""
        with tempfile.TemporaryDirectory() as tmp:
            path = _write_registry(tmp, body)
            errors = validate_protocol_registry.validate_field_model(path)
            self.assertTrue(
                any("unrecognized type" in e for e in errors),
                msg=errors,
            )

    def test_mirror_drift_fails(self) -> None:
        # `mirror_of` legacy values diverge from enum values.
        body = """
setup_levels:
  - session
  - file
enums:
  setup_level:
    description: Lifecycle scope.
    values: [session, file, function]
    mirror_of: setup_levels
commands:
  shutdown:
    description: Shutdown.
    fields: []
    response_status: shutdown_ack
    field_model:
      request_fields: {}
      response_fields: {}
"""
        with tempfile.TemporaryDirectory() as tmp:
            path = _write_registry(tmp, body)
            errors = validate_protocol_registry.validate_field_model(path)
            self.assertTrue(
                any("drift from legacy" in e for e in errors),
                msg=errors,
            )

    def test_array_and_ref_types_recognized(self) -> None:
        body = """
enums: {}
commands:
  analyze:
    description: Analyze.
    fields: [file, things]
    response_status: analyze
    field_model:
      request_fields:
        file: { type: string, optional: false }
        things: { type: "array<ref:thing.schema.json>", optional: true }
      response_fields:
        out: { type: array<integer>, optional: false }
"""
        with tempfile.TemporaryDirectory() as tmp:
            path = _write_registry(tmp, body)
            errors = validate_protocol_registry.validate_field_model(path)
            self.assertEqual(errors, [], msg=errors)


class SourceNameLayerTest(unittest.TestCase):
    """Tests for the source-name layer: frontend vocabulary + implemented-commands checks."""

    def test_ts_vocab_extraction_nonempty_on_live_repo(self) -> None:
        # Regression guard for str-qwua7.7: extract_ts previously read
        # protocol.ts for a `type Command = ...` union that no longer
        # exists there (it moved to the generated module in str-1hlk.7),
        # so extraction silently returned empty sets and the check no-op'd.
        vocab = validate_protocol_registry.extract_ts_vocab(
            validate_protocol_registry.TS_GENERATED_ENUMS
        )
        self.assertIn("analyze", vocab["commands"])
        self.assertIn("execute", vocab["statuses"])
        self.assertTrue(vocab["error_codes"])

    def test_ts_implemented_commands_nonempty_on_live_repo(self) -> None:
        implemented = validate_protocol_registry.extract_ts_implemented_commands(
            validate_protocol_registry.TS_HANDLERS
        )
        self.assertIn("analyze", implemented)
        self.assertIn("shutdown", implemented)

    def test_go_vocab_and_implemented_commands_nonempty_on_live_repo(self) -> None:
        vocab = validate_protocol_registry.extract_go_vocab(
            validate_protocol_registry.GO_GENERATED_ENUMS
        )
        self.assertIn("analyze", vocab["commands"])
        implemented = validate_protocol_registry.extract_go_implemented_commands(
            validate_protocol_registry.GO_HANDLER
        )
        self.assertIn("analyze", implemented)

    def test_rust_fe_vocab_and_implemented_commands_nonempty_on_live_repo(self) -> None:
        # Regression guard for str-qwua7.7: the old extract_rust_fe used a
        # hard-coded command alternation that omitted "prepare" and
        # "get_invocation_plan", so implemented commands like "prepare"
        # were falsely reported as missing.
        vocab = validate_protocol_registry.extract_rust_fe_vocab(
            validate_protocol_registry.RUST_FE_GENERATED_ENUMS
        )
        self.assertIn("prepare", vocab["commands"])
        implemented = validate_protocol_registry.extract_rust_fe_implemented_commands(
            validate_protocol_registry.RUST_FE_HANDLER
        )
        self.assertIn("prepare", implemented)
        self.assertIn("instrument", implemented)
        self.assertIn("shutdown", implemented)

    def test_removed_command_in_rust_handler_is_detected(self) -> None:
        """Deliberately drop a dispatched command from a temp copy of handler.rs
        and confirm validate_implemented_commands reports it."""
        text = validate_protocol_registry.RUST_FE_HANDLER.read_text()
        mutated = text.replace(
            '"teardown" => (self.handle_teardown(resp, req), false),\n', ""
        )
        self.assertNotEqual(mutated, text, "fixture line not found in handler.rs")

        with tempfile.TemporaryDirectory() as tmp:
            mutated_path = Path(tmp) / "handler.rs"
            mutated_path.write_text(mutated)
            implemented = validate_protocol_registry.extract_rust_fe_implemented_commands(
                mutated_path
            )
        self.assertNotIn("teardown", implemented)

        registry = validate_protocol_registry.parse_registry(LIVE_REGISTRY)
        issues = validate_protocol_registry.validate_implemented_commands(
            registry, "shatter-rust", implemented
        )
        self.assertTrue(
            any("'teardown'" in issue for issue in issues),
            msg=issues,
        )

    def test_empty_extraction_is_reported_as_error(self) -> None:
        """An extractor that finds nothing must fail loud, not silently pass."""
        with tempfile.TemporaryDirectory() as tmp:
            broken_path = Path(tmp) / "protocol-enums.ts"
            broken_path.write_text("// no ALL_COMMANDS here\n")
            vocab = validate_protocol_registry.extract_ts_vocab(broken_path)
            error = validate_protocol_registry._require_nonempty(
                vocab, broken_path, "extract_ts_vocab"
            )
        self.assertIsNotNone(error)
        assert error is not None
        self.assertIn(str(broken_path), error)
        self.assertIn("extract_ts_vocab", error)

    def test_live_repo_passes_full_validation(self) -> None:
        """End-to-end: running the script against the checked-in repo must exit 0."""
        result = validate_protocol_registry.main()
        self.assertEqual(result, 0)


class BracedBlockCommentAndStringBraceTest(unittest.TestCase):
    """A `{`/`}` inside a comment or string literal must not desync brace
    balance counting and swallow code past the real end of the dispatch
    block (str-qwua7.7 review finding on `_extract_braced_block`)."""

    RUST_FIXTURE = """
fn dispatch(&mut self, req: &Request) {
    match req.command.as_str() {
        // only match one { at a time
        "handshake" => (self.handle_handshake(resp, req), false),
        "shutdown" => (self.handle_shutdown(resp), true),
        _ => unreachable(),
    }
}

fn unrelated(&self) {
    match other.as_str() {
        "bogus_outside" => 1,
        _ => 0,
    }
}
"""

    TS_FIXTURE = """
function dispatch(request) {
  switch (request.command) {
    // only match one { at a time
    case "handshake":
      return handleHandshake();
    case "shutdown":
      return handleShutdown();
  }
}

function unrelated(other) {
  switch (other) {
    case "bogus_outside":
      return 1;
  }
}
"""

    GO_FIXTURE = """
func dispatch(req Request) {
	switch req.Command {
	// only match one { at a time
	case "handshake":
		return handleHandshake()
	case "shutdown":
		return handleShutdown()
	}
}

func unrelated(other string) {
	switch other {
	case "bogus_outside":
		return
	}
}
"""

    def _write(self, tmp_dir: str, name: str, body: str) -> Path:
        path = Path(tmp_dir) / name
        path.write_text(body)
        return path

    def test_rust_comment_brace_does_not_leak_across_match_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write(tmp, "handler.rs", self.RUST_FIXTURE)
            implemented = validate_protocol_registry.extract_rust_fe_implemented_commands(path)
        self.assertEqual(implemented, {"handshake", "shutdown"})
        self.assertNotIn("bogus_outside", implemented)

    def test_ts_comment_brace_does_not_leak_across_switch_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write(tmp, "handlers.ts", self.TS_FIXTURE)
            implemented = validate_protocol_registry.extract_ts_implemented_commands(path)
        self.assertEqual(implemented, {"handshake", "shutdown"})
        self.assertNotIn("bogus_outside", implemented)

    def test_go_comment_brace_does_not_leak_across_switch_blocks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write(tmp, "handler.go", self.GO_FIXTURE)
            implemented = validate_protocol_registry.extract_go_implemented_commands(path)
        self.assertEqual(implemented, {"handshake", "shutdown"})
        self.assertNotIn("bogus_outside", implemented)

    def test_block_comment_with_stray_quote_does_not_corrupt_string_masking(self) -> None:
        # str-qwua7.7 review round 2: masking strings before comments (or
        # comments before strings, via two separate global passes) lets a
        # quote inside a `/* */` comment pair with a later real quote and
        # eat the comment's own "*/" terminator, corrupting everything
        # after it on that line and silently dropping the real command.
        fixture = """
fn dispatch(&mut self, req: &Request) {
    match req.command.as_str() {
        /* say "hi */
        "handshake" => (self.handle_handshake(resp, req), false),
        "shutdown" => (self.handle_shutdown(resp), true),
        _ => unreachable(),
    }
}

fn unrelated(&self) {
    match other.as_str() {
        "bogus_outside" => 1,
        _ => 0,
    }
}
"""
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write(tmp, "handler.rs", fixture)
            implemented = validate_protocol_registry.extract_rust_fe_implemented_commands(path)
        self.assertEqual(implemented, {"handshake", "shutdown"})
        self.assertNotIn("bogus_outside", implemented)

    def test_string_literal_brace_does_not_desync_block_boundary(self) -> None:
        # A `{` inside a string literal (e.g. an error message) must not be
        # counted either.
        fixture = """
fn dispatch(&mut self, req: &Request) {
    match req.command.as_str() {
        "handshake" => log("unexpected { in payload"),
        "shutdown" => (self.handle_shutdown(resp), true),
        _ => unreachable(),
    }
}

fn unrelated(&self) {
    match other.as_str() {
        "bogus_outside" => 1,
        _ => 0,
    }
}
"""
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write(tmp, "handler.rs", fixture)
            implemented = validate_protocol_registry.extract_rust_fe_implemented_commands(path)
        self.assertEqual(implemented, {"handshake", "shutdown"})
        self.assertNotIn("bogus_outside", implemented)


if __name__ == "__main__":
    unittest.main()
