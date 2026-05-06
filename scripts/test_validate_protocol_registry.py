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


if __name__ == "__main__":
    unittest.main()
