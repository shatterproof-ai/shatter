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


class RustFrontendCommandExtractionTest(unittest.TestCase):
    """Regression coverage for str-qwua7.7: extract_rust_fe's commands must
    come from handler.rs's real dispatch arms, not a hard-coded regex
    alternation that silently omits newly dispatched commands."""

    def test_dispatched_commands_all_found_on_live_repo(self) -> None:
        # Before this fix, a hard-coded alternation omitted "prepare" and
        # "get_invocation_plan" (which is legitimately unimplemented), so
        # "prepare" was falsely reported as missing even though handler.rs
        # dispatches it.
        result = validate_protocol_registry.extract_rust_fe(
            validate_protocol_registry.RUST_FE_PROTOCOL,
            validate_protocol_registry.RUST_FE_HANDLER,
        )
        self.assertIn("prepare", result["commands"])
        self.assertIn("instrument", result["commands"])
        self.assertIn("shutdown", result["commands"])

    def test_new_dispatch_arm_is_picked_up_without_updating_this_script(self) -> None:
        """A command dispatched via a new match arm must be extracted even
        though it isn't hard-coded anywhere in the extractor."""
        with tempfile.TemporaryDirectory() as tmp:
            handler_path = Path(tmp) / "handler.rs"
            handler_path.write_text(
                'match req.command.as_str() {\n'
                '    "handshake" => (self.handle_handshake(resp, req), false),\n'
                '    "brand_new_command" => (self.handle_new(resp, req), false),\n'
                "    _ => unreachable!(),\n"
                "}\n"
            )
            protocol_path = Path(tmp) / "protocol.rs"
            protocol_path.write_text("")
            result = validate_protocol_registry.extract_rust_fe(
                protocol_path, handler_path
            )
        self.assertIn("brand_new_command", result["commands"])


if __name__ == "__main__":
    unittest.main()
