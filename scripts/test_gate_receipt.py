"""Contract tests for the gate receipt v1 writer (str-35vtk.22)."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import time
import unittest

import yaml


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "gate-receipt.py"
SPEC = ROOT / "docs" / "perf" / "gate-receipt-v1.md"
TASKFILE = ROOT / "Taskfile.yml"

EXIT_OK = 0
EXIT_INVALID = 64
EXIT_NOT_VALID = 65
EXIT_IOERR = 74
LOCK_PATHS = (
    "Cargo.lock",
    "shatter-go/go.sum",
    "shatter-rust-runtime/Cargo.lock",
    "shatter-rust/Cargo.lock",
    "shatter-ts/package-lock.json",
)
TOOL_OUTPUTS = {
    "cargo": "cargo 1.91.0 (fixture)",
    "go": "go version go1.25.0 fixture",
    "node": "v24.1.0",
    "npm": "11.4.0",
    "rustc": "rustc 1.91.0 (fixture)",
    "task": "Task version: v3.44.1",
}
GOLDEN_BASE_TREE = "2f198f702c63dc2433b190dd67780712a3a5a229"
GOLDEN_CANDIDATE_TREE = "b18f574cba7a16806bbcc4808f45b0ed9adf66d4"
GOLDEN_DIGEST = "sha256:60b989d5115baeafe6fea2de8bc77ee733aad6eb884df23d8b97f2122457de65"


def canonical_json(value: object) -> bytes:
    """Independent test encoder for receipt bytes."""
    return json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
        allow_nan=False,
    ).encode("utf-8")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


class ReceiptRepo:
    """Disposable Git repository with deterministic candidate-tree blobs."""

    CODE_BYTES = {
        "Taskfile.yml": b"version: '3'\ntasks: {}\n",
        "nested/BuildTaskfile.ci.yml": "version: '3'\n# café\n".encode(),
        "scripts/gate-wrapper.sh": b"#!/bin/sh\nexec \"$@\"\n",
        "scripts/gate-receipt.py": b"#!/usr/bin/env python3\n# candidate fixture\n",
    }
    LOCK_BYTES = {
        "Cargo.lock": b"# fixture cargo lock\n",
        "shatter-ts/package-lock.json": b'{"lockfileVersion":3}\n',
        "shatter-go/go.sum": b"example.test/mod v1.0.0 h1:fixture=\n",
    }

    def __init__(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.repo = self.root / "repo"
        self.runtime = self.root / "runtime"
        self.fake_bin = self.root / "bin"
        self.repo.mkdir()
        self.runtime.mkdir(mode=0o700)
        self.fake_bin.mkdir()
        self._git("init", "--quiet")

        self._write("Taskfile.yml", b"version: '3'\ntasks: {base: {}}\n")
        self._write("scripts/gate-wrapper.sh", b"#!/bin/sh\nexit 0\n", executable=True)
        self._write("scripts/gate-receipt.py", b"#!/usr/bin/env python3\n# base\n", executable=True)
        self._git("add", "-A")
        self.base = self._git("write-tree").stdout.strip()

        for path, data in {**self.CODE_BYTES, **self.LOCK_BYTES}.items():
            self._write(path, data, executable=path.startswith("scripts/"))
        self._git("add", "-A")
        self.candidate = self._git("write-tree").stdout.strip()

        self.result_a = self._result(
            "zeta", ["task", "check"], "2026-08-30T12:00:02Z", "2026-08-30T12:01:00Z"
        )
        self.result_b = self._result(
            "alpha", ["task", "meta", "--", "café"], "2026-08-30T12:00:00Z", "2026-08-30T12:02:03Z"
        )
        self._install_fake_tools()

    def close(self) -> None:
        self._tmp.cleanup()

    def _git(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["git", *args], cwd=self.repo, text=True, capture_output=True, check=True
        )

    def _write(self, relative: str, data: bytes, *, executable: bool = False) -> None:
        path = self.repo / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        if executable:
            path.chmod(0o755)

    def _result(
        self, gate: str, argv: list[str], started_at: str, ended_at: str
    ) -> Path:
        path = self.root / f"{gate}.json"
        path.write_text(
            json.dumps(
                {
                    "gate": gate,
                    "argv": argv,
                    "started_at": started_at,
                    "ended_at": ended_at,
                    "exit_code": 0,
                }
            )
        )
        return path

    def _install_fake_tools(self) -> None:
        real_git = subprocess.run(
            ["sh", "-c", "command -v git"], text=True, capture_output=True, check=True
        ).stdout.strip()
        git_shim = self.fake_bin / "git"
        git_shim.write_text(f'#!/bin/sh\nexec "{real_git}" "$@"\n')
        git_shim.chmod(0o755)
        for name, output in TOOL_OUTPUTS.items():
            tool = self.fake_bin / name
            expected_arg = "version" if name == "go" else "--version"
            tool.write_text(
                "#!/bin/sh\n"
                f'[ "$#" -eq 1 ] && [ "$1" = "{expected_arg}" ] || exit 3\n'
                f"printf '  %s  \\n' '{output}'\n"
            )
            tool.chmod(0o755)

    @property
    def env(self) -> dict[str, str]:
        env = dict(os.environ)
        env.update({"PATH": str(self.fake_bin), "XDG_RUNTIME_DIR": str(self.runtime)})
        for key in (
            "GIT_DIR",
            "GIT_COMMON_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        ):
            env.pop(key, None)
        return env

    def run(
        self,
        *,
        candidate: str | None = None,
        base: str | None = None,
        tier: str = "local",
        results: tuple[Path, ...] | None = None,
        env: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        args = [
            sys.executable,
            str(SCRIPT),
            "write",
            "--candidate",
            candidate or self.candidate,
            "--base",
            base or self.base,
            "--tier",
            tier,
        ]
        selected = (self.result_a, self.result_b) if results is None else results
        for result in selected:
            args.extend(("--gate-result", str(result)))
        return subprocess.run(
            args,
            cwd=self.repo,
            env=env or self.env,
            text=True,
            capture_output=True,
            check=False,
        )

    def requirements(self, gates: list[dict[str, object]] | None = None) -> Path:
        path = self.root / "requirements.json"
        path.write_text(
            json.dumps(
                {
                    "schema": 1,
                    "requirements": gates
                    if gates is not None
                    else [
                        {"gate": "alpha", "argv": ["task", "meta", "--", "café"]},
                        {"gate": "zeta", "argv": ["task", "check"]},
                    ],
                }
            )
        )
        return path

    def validate(
        self,
        *,
        candidate: str | None = None,
        base: str | None = None,
        tier: str = "local",
        requirements: Path | None = None,
        path: Path | None = None,
        env: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        args = [
            sys.executable,
            str(SCRIPT),
            "validate",
            "--candidate",
            candidate or self.candidate,
            "--base",
            base or self.base,
            "--tier",
            tier,
            "--requirements",
            str(requirements or self.requirements()),
        ]
        if path is not None:
            args.extend(("--path", str(path)))
        return subprocess.run(
            args,
            cwd=self.repo,
            env=env or self.env,
            text=True,
            capture_output=True,
            check=False,
        )

    def mutate_receipt(self, mutate, *, recompute_digest: bool = True) -> None:
        receipt = json.loads(self.receipt_path.read_text())
        mutate(receipt)
        if recompute_digest:
            receipt.pop("digest", None)
            receipt["digest"] = f"sha256:{sha256(canonical_json(receipt))}"
        self.receipt_path.write_bytes(canonical_json(receipt) + b"\n")
        self.receipt_path.chmod(0o600)

    @property
    def receipt_path(self) -> Path:
        common_dir = Path(self._git("rev-parse", "--git-common-dir").stdout.strip())
        if not common_dir.is_absolute():
            common_dir = self.repo / common_dir
        repo_key = sha256(os.fsencode(common_dir.resolve()))
        return self.runtime / "shatter-gate-receipts" / "v1" / repo_key / f"{self.candidate}.json"


class ReceiptTestCase(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture = ReceiptRepo()

    def tearDown(self) -> None:
        self.fixture.close()


class ReceiptSurfaceTests(unittest.TestCase):
    def test_writer_script_exists_and_is_executable(self) -> None:
        self.assertTrue(SCRIPT.is_file(), f"missing {SCRIPT}")
        self.assertTrue(SCRIPT.stat().st_mode & 0o111, f"not executable: {SCRIPT}")

    def test_normative_spec_records_complete_contract(self) -> None:
        text = SPEC.read_text()
        for phrase in (
            "Gate Receipt v1", "candidate_tree", "base_tree", "keys sorted recursively",
            "ensure_ascii=False", "cargo --version", "go version", "scripts/gate-wrapper.sh",
            "scripts/gate-receipt.py", "shatter-gate-receipts/v1", "mode 0700",
            "mode 0600", "atomically",
        ):
            with self.subTest(phrase=phrase):
                self.assertIn(phrase, text)

    def test_meta_runs_receipt_contract_tests(self) -> None:
        tasks = yaml.safe_load(TASKFILE.read_text())["tasks"]
        for source in (
            "scripts/gate-receipt.py", "scripts/test_gate_receipt.py", "docs/perf/gate-receipt-v1.md",
        ):
            self.assertIn(source, tasks["meta"]["sources"])
        self.assertIn("python3 -m unittest scripts.test_gate_receipt", tasks["meta"]["cmds"])


class GoldenReceiptTests(ReceiptTestCase):
    def expected_without_digest(self) -> dict[str, object]:
        code = [
            {"path": path, "sha256": sha256(data)}
            for path, data in sorted(self.fixture.CODE_BYTES.items())
        ]
        locks = [
            {
                "path": path,
                "present": path in self.fixture.LOCK_BYTES,
                "sha256": sha256(self.fixture.LOCK_BYTES[path]) if path in self.fixture.LOCK_BYTES else None,
            }
            for path in LOCK_PATHS
        ]
        return {
            "schema": 1,
            "candidate_tree": self.fixture.candidate,
            "base_tree": self.fixture.base,
            "tier": "local",
            "gate_results": [
                {"gate": "alpha", "argv": ["task", "meta", "--", "café"], "started_at": "2026-08-30T12:00:00Z", "ended_at": "2026-08-30T12:02:03Z", "exit_code": 0},
                {"gate": "zeta", "argv": ["task", "check"], "started_at": "2026-08-30T12:00:02Z", "ended_at": "2026-08-30T12:01:00Z", "exit_code": 0},
            ],
            "bindings": {"code": code, "locks": locks},
            "tools": [
                {"name": name, "version": version} for name, version in sorted(TOOL_OUTPUTS.items())
            ],
            "started_at": "2026-08-30T12:00:00Z",
            "ended_at": "2026-08-30T12:02:03Z",
        }

    def test_golden_bytes_digest_stdout_and_modes(self) -> None:
        result = self.fixture.run()
        self.assertEqual(result.returncode, EXIT_OK, result.stderr)
        self.assertEqual(result.stderr, "")
        self.assertEqual(self.fixture.base, GOLDEN_BASE_TREE)
        self.assertEqual(self.fixture.candidate, GOLDEN_CANDIDATE_TREE)
        expected = self.expected_without_digest()
        digest = f"sha256:{sha256(canonical_json(expected))}"
        self.assertEqual(digest, GOLDEN_DIGEST)
        expected["digest"] = digest
        self.assertEqual(self.fixture.receipt_path.read_bytes(), canonical_json(expected) + b"\n")
        self.assertEqual(
            json.loads(result.stdout),
            {"status": "written", "path": str(self.fixture.receipt_path), "digest": digest},
        )
        self.assertEqual(stat.S_IMODE(self.fixture.receipt_path.stat().st_mode), 0o600)
        receipt_root = self.fixture.runtime / "shatter-gate-receipts"
        for directory in (receipt_root, receipt_root / "v1", self.fixture.receipt_path.parent):
            self.assertEqual(stat.S_IMODE(directory.stat().st_mode), 0o700)

    def test_dirty_and_untracked_files_do_not_change_receipt(self) -> None:
        first = self.fixture.run()
        self.assertEqual(first.returncode, EXIT_OK, first.stderr)
        before = self.fixture.receipt_path.read_bytes()
        self.fixture._write("Taskfile.yml", b"dirty tracked bytes\n")
        self.fixture._write("untracked/OtherTaskfile.dev.yml", b"untracked\n")
        second = self.fixture.run()
        self.assertEqual(second.returncode, EXIT_OK, second.stderr)
        self.assertEqual(self.fixture.receipt_path.read_bytes(), before)


class InputRejectionTests(ReceiptTestCase):
    def assert_invalid(self, result: subprocess.CompletedProcess[str]) -> None:
        self.assertEqual(result.returncode, EXIT_INVALID, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertFalse(self.fixture.receipt_path.exists())
        self.assertNotIn("Traceback", result.stderr)

    def test_invalid_candidate_tree_forms_and_non_tree_objects(self) -> None:
        blob = self.fixture._git("hash-object", "Taskfile.yml").stdout.strip()
        for candidate in ("abc", "A" * 40, "g" * 40, "0" * 40, blob):
            with self.subTest(candidate=candidate):
                self.assert_invalid(self.fixture.run(candidate=candidate))

    def test_commit_oid_is_not_accepted_as_a_tree(self) -> None:
        env = dict(os.environ, GIT_AUTHOR_NAME="Fixture", GIT_AUTHOR_EMAIL="f@example.test")
        env.update(GIT_COMMITTER_NAME="Fixture", GIT_COMMITTER_EMAIL="f@example.test")
        commit = subprocess.run(
            ["git", "commit-tree", self.fixture.candidate, "-m", "fixture"],
            cwd=self.fixture.repo, env=env, text=True, capture_output=True, check=True,
        ).stdout.strip()
        self.assert_invalid(self.fixture.run(candidate=commit))

    def test_invalid_base_tree_is_rejected(self) -> None:
        self.assert_invalid(self.fixture.run(base="0" * 40))

    def test_gate_result_schema_timestamp_exit_and_uniqueness(self) -> None:
        bad_values = (
            [],
            {"gate": "", "argv": ["task"], "started_at": "2026-08-30T00:00:00Z", "ended_at": "2026-08-30T00:00:01Z", "exit_code": 0},
            {"gate": "x", "argv": [], "started_at": "2026-08-30T00:00:00Z", "ended_at": "2026-08-30T00:00:01Z", "exit_code": 0},
            {"gate": "x", "argv": ["task"], "started_at": "2026-08-30T00:00:00.1Z", "ended_at": "2026-08-30T00:00:01Z", "exit_code": 0},
            {"gate": "x", "argv": ["task"], "started_at": "2026-08-30T00:00:02Z", "ended_at": "2026-08-30T00:00:01Z", "exit_code": 0},
            {"gate": "x", "argv": ["task"], "started_at": "2026-08-30T00:00:00Z", "ended_at": "2026-08-30T00:00:01Z", "exit_code": True},
            {"gate": "x", "argv": ["task"], "started_at": "2026-08-30T00:00:00Z", "ended_at": "2026-08-30T00:00:01Z", "exit_code": 1},
            {"gate": "x", "argv": ["task"], "started_at": "2026-08-30T00:00:00Z", "ended_at": "2026-08-30T00:00:01Z", "exit_code": 0, "extra": 1},
        )
        for index, value in enumerate(bad_values):
            path = self.fixture.root / f"bad-{index}.json"
            path.write_text(json.dumps(value))
            with self.subTest(value=value):
                self.assert_invalid(self.fixture.run(results=(path,)))
        self.assert_invalid(self.fixture.run(results=()))
        self.assert_invalid(self.fixture.run(results=(self.fixture.result_a, self.fixture.result_a)))

    def test_duplicate_json_keys_are_invalid(self) -> None:
        path = self.fixture.root / "duplicate.json"
        path.write_text(
            '{"gate":"a","gate":"b","argv":["task"],'
            '"started_at":"2026-08-30T00:00:00Z",'
            '"ended_at":"2026-08-30T00:00:01Z","exit_code":0}'
        )
        self.assert_invalid(self.fixture.run(results=(path,)))

    def test_invalid_utf8_gate_result_is_invalid(self) -> None:
        path = self.fixture.root / "invalid-utf8.json"
        path.write_bytes(b"\xff")
        self.assert_invalid(self.fixture.run(results=(path,)))

    def test_missing_required_candidate_code_is_invalid(self) -> None:
        self.fixture._git("update-index", "--force-remove", "scripts/gate-wrapper.sh")
        tree = self.fixture._git("write-tree").stdout.strip()
        self.assert_invalid(self.fixture.run(candidate=tree))

    def test_bad_cli_invocation_uses_exit_64(self) -> None:
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "write"], cwd=self.fixture.repo,
            env=self.fixture.env, text=True, capture_output=True, check=False,
        )
        self.assertEqual(result.returncode, EXIT_INVALID, result.stderr)
        self.assertEqual(result.stdout, "")


class DiscoveryAndIOTests(ReceiptTestCase):
    def test_missing_tool_is_discovery_error_without_receipt(self) -> None:
        (self.fixture.fake_bin / "cargo").unlink()
        result = self.fixture.run()
        self.assertEqual(result.returncode, EXIT_IOERR, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertFalse(self.fixture.receipt_path.exists())

    def test_missing_gate_result_file_is_io_error(self) -> None:
        result = self.fixture.run(results=(self.fixture.root / "missing.json",))
        self.assertEqual(result.returncode, EXIT_IOERR, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertFalse(self.fixture.receipt_path.exists())

    def test_non_directory_runtime_path_is_io_error(self) -> None:
        blocked = self.fixture.root / "blocked"
        blocked.write_text("not a directory")
        env = self.fixture.env
        env["XDG_RUNTIME_DIR"] = str(blocked)
        result = self.fixture.run(env=env)
        self.assertEqual(result.returncode, EXIT_IOERR, result.stderr)
        self.assertEqual(result.stdout, "")


class ValidatorRoundTripTests(ReceiptTestCase):
    def setUp(self) -> None:
        super().setUp()
        written = self.fixture.run()
        self.assertEqual(written.returncode, EXIT_OK, written.stderr)

    def test_exact_roundtrip_is_valid_and_read_only(self) -> None:
        requirements = self.fixture.requirements()
        receipt_before = self.fixture.receipt_path.read_bytes()
        requirements_before = requirements.read_bytes()
        receipt_mtime = self.fixture.receipt_path.stat().st_mtime_ns
        requirements_mtime = requirements.stat().st_mtime_ns

        result = self.fixture.validate(requirements=requirements)

        self.assertEqual(result.returncode, EXIT_OK, result.stderr)
        self.assertEqual(result.stdout, '{"reasons":[],"status":"valid"}\n')
        self.assertEqual(result.stderr, "")
        self.assertEqual(self.fixture.receipt_path.read_bytes(), receipt_before)
        self.assertEqual(requirements.read_bytes(), requirements_before)
        self.assertEqual(self.fixture.receipt_path.stat().st_mtime_ns, receipt_mtime)
        self.assertEqual(requirements.stat().st_mtime_ns, requirements_mtime)

    def test_explicit_path_is_valid_without_requiring_private_parent(self) -> None:
        custom_parent = self.fixture.root / "caller-owned"
        custom_parent.mkdir(mode=0o755)
        custom = custom_parent / "receipt.json"
        custom.write_bytes(self.fixture.receipt_path.read_bytes())
        custom.chmod(0o600)

        result = self.fixture.validate(path=custom)

        self.assertEqual(result.returncode, EXIT_OK, result.stderr)
        self.assertEqual(json.loads(result.stdout), {"status": "valid", "reasons": []})


class ValidatorReasonTests(ReceiptTestCase):
    def setUp(self) -> None:
        super().setUp()
        written = self.fixture.run()
        self.assertEqual(written.returncode, EXIT_OK, written.stderr)
        self.original = self.fixture.receipt_path.read_bytes()

    def reset_receipt(self) -> None:
        self.fixture.receipt_path.write_bytes(self.original)
        self.fixture.receipt_path.chmod(0o600)

    def assert_reasons(
        self,
        expected: list[str],
        *,
        requirements: Path | None = None,
    ) -> None:
        result = self.fixture.validate(requirements=requirements)
        self.assertEqual(result.returncode, EXIT_NOT_VALID, result.stderr)
        self.assertEqual(result.stderr, "")
        self.assertEqual(json.loads(result.stdout), {"status": "invalid", "reasons": expected})

    def test_each_invalid_reason(self) -> None:
        cases = (
            ("permissions", lambda receipt: None),
            ("digest", lambda receipt: receipt.__setitem__("digest", "sha256:" + "0" * 64)),
            ("tree", lambda receipt: receipt.__setitem__("candidate_tree", self.fixture.base)),
            ("base", lambda receipt: receipt.__setitem__("base_tree", self.fixture.candidate)),
            ("code", lambda receipt: receipt["bindings"]["code"][0].__setitem__("sha256", "0" * 64)),
            ("lock", lambda receipt: receipt["bindings"]["locks"][0].__setitem__("sha256", "0" * 64)),
            ("tool", lambda receipt: receipt["tools"][0].__setitem__("version", "other")),
            ("tier", lambda receipt: receipt.__setitem__("tier", "ci")),
            ("duplicate_gate", lambda receipt: receipt["gate_results"].append(dict(receipt["gate_results"][0]))),
            ("failed_gate", lambda receipt: receipt["gate_results"][0].__setitem__("exit_code", 1)),
        )
        for reason, mutate in cases:
            with self.subTest(reason=reason):
                self.reset_receipt()
                self.fixture.mutate_receipt(mutate, recompute_digest=reason != "digest")
                if reason == "permissions":
                    self.fixture.receipt_path.chmod(0o644)
                self.assert_reasons([reason])

        self.reset_receipt()
        missing = self.fixture.requirements([{"gate": "missing", "argv": ["task", "missing"]}])
        self.assert_reasons(["missing_gate"], requirements=missing)

        self.reset_receipt()
        argv = self.fixture.requirements(
            [{"gate": "alpha", "argv": ["task", "different"]}]
        )
        self.assert_reasons(["argv"], requirements=argv)

    def test_malformed_receipt_and_requirements(self) -> None:
        self.fixture.receipt_path.write_text("{}\n")
        self.fixture.receipt_path.chmod(0o600)
        self.assert_reasons(["malformed"])

        self.reset_receipt()
        requirements = self.fixture.root / "bad-requirements.json"
        requirements.write_text('{"schema":1,"requirements":[]} trailing')
        self.assert_reasons(["malformed"], requirements=requirements)

    def test_multi_reason_output_uses_fixed_order(self) -> None:
        def mutate(receipt: dict[str, object]) -> None:
            receipt["candidate_tree"] = self.fixture.base
            receipt["base_tree"] = self.fixture.candidate
            receipt["bindings"]["code"][0]["sha256"] = "0" * 64
            receipt["tier"] = "ci"
            duplicate = dict(receipt["gate_results"][0])
            duplicate["exit_code"] = 1
            duplicate["argv"] = ["task", "wrong"]
            receipt["gate_results"].append(duplicate)

        self.fixture.mutate_receipt(mutate)
        receipt = json.loads(self.fixture.receipt_path.read_text())
        receipt["digest"] = "sha256:" + "0" * 64
        self.fixture.receipt_path.write_bytes(canonical_json(receipt) + b"\n")
        self.fixture.receipt_path.chmod(0o644)
        requirements = self.fixture.requirements(
            [
                {"gate": "alpha", "argv": ["task", "expected"]},
                {"gate": "missing", "argv": ["task", "missing"]},
            ]
        )
        self.assert_reasons(
            [
                "permissions",
                "digest",
                "tree",
                "base",
                "code",
                "tier",
                "duplicate_gate",
                "missing_gate",
                "failed_gate",
                "argv",
            ],
            requirements=requirements,
        )

    def test_loose_default_directory_mode_is_permissions_reason(self) -> None:
        self.fixture.receipt_path.parent.chmod(0o755)
        self.assert_reasons(["permissions"])


class ValidatorDiscoveryTests(ReceiptTestCase):
    def test_missing_tool_is_exit_74_not_invalid_receipt(self) -> None:
        written = self.fixture.run()
        self.assertEqual(written.returncode, EXIT_OK, written.stderr)
        (self.fixture.fake_bin / "cargo").unlink()

        result = self.fixture.validate()

        self.assertEqual(result.returncode, EXIT_IOERR, result.stderr)
        self.assertEqual(result.stdout, "")

    def test_missing_receipt_and_requirements_are_io_errors(self) -> None:
        missing_requirements = self.fixture.root / "missing-requirements.json"
        result = self.fixture.validate(requirements=missing_requirements)
        self.assertEqual(result.returncode, EXIT_IOERR, result.stderr)
        self.assertEqual(result.stdout, "")

        result = self.fixture.validate(path=self.fixture.root / "missing-receipt.json")
        self.assertEqual(result.returncode, EXIT_IOERR, result.stderr)
        self.assertEqual(result.stdout, "")


class AtomicReplacementTests(ReceiptTestCase):
    def test_concurrent_readers_observe_only_complete_receipts(self) -> None:
        initial = self.fixture.run(tier="local")
        self.assertEqual(initial.returncode, EXIT_OK, initial.stderr)
        old = self.fixture.receipt_path.read_bytes()
        expected_variants = {old}
        for tier, result_path in (("ci", self.fixture.result_b), ("local", self.fixture.result_a)):
            generated = self.fixture.run(tier=tier, results=(result_path,))
            self.assertEqual(generated.returncode, EXIT_OK, generated.stderr)
            expected_variants.add(self.fixture.receipt_path.read_bytes())
        self.fixture.receipt_path.write_bytes(old)
        self.fixture.receipt_path.chmod(0o600)
        commands = []
        for tier, result_path in (("ci", self.fixture.result_b), ("local", self.fixture.result_a)):
            commands.append([
                sys.executable, str(SCRIPT), "write", "--candidate", self.fixture.candidate,
                "--base", self.fixture.base, "--tier", tier, "--gate-result", str(result_path),
            ])
        processes = [
            subprocess.Popen(
                command, cwd=self.fixture.repo, env=self.fixture.env,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            )
            for command in commands * 3
        ]
        observed: list[bytes] = []
        while any(process.poll() is None for process in processes):
            observed.append(self.fixture.receipt_path.read_bytes())
            time.sleep(0.001)
        for process in processes:
            stdout, stderr = process.communicate()
            self.assertEqual(process.returncode, EXIT_OK, stderr)
            self.assertTrue(stdout)
        observed.append(self.fixture.receipt_path.read_bytes())
        self.assertTrue(observed)
        self.assertTrue(all(value in expected_variants for value in observed))
        self.assertFalse(list(self.fixture.receipt_path.parent.glob("*.tmp")))


if __name__ == "__main__":
    unittest.main()
