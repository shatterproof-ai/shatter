from __future__ import annotations

import importlib.util
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "scripts" / "init_project.py"
SPEC = importlib.util.spec_from_file_location("init_project", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


FIXTURES_DIR = Path(__file__).resolve().parent / "fixtures" / "init-project"


class InitProjectTest(unittest.TestCase):
    def copy_fixture(self, name: str) -> Path:
        tempdir = Path(tempfile.mkdtemp(prefix="shatter-init-project-"))
        self.addCleanup(lambda: shutil.rmtree(tempdir, ignore_errors=True))
        shutil.copytree(FIXTURES_DIR / name, tempdir / name)
        return tempdir / name

    def test_prefers_taskfile_when_docs_point_to_task(self) -> None:
        repo = self.copy_fixture("taskfile-preferred")
        report = MODULE.DISCOVER_MODULE.build_report(repo)
        results = MODULE.build_results(report, apply=True, skip_init=True, command="shatter")

        integrated = {entry["root"]: entry for entry in results["targets"]}
        app = integrated["app"]
        self.assertEqual(app["status"], "integrated")
        self.assertEqual(app["selected_surface"]["type"], "taskfile")
        self.assertTrue(app["wrapper_updated"])
        self.assertIn("shatter:", (repo / "app" / "Taskfile.yml").read_text(encoding="utf-8"))

        package = json.loads((repo / "app" / "package.json").read_text(encoding="utf-8"))
        self.assertNotIn("shatter", package["scripts"])

    def test_updates_package_json_when_it_is_the_only_local_surface(self) -> None:
        repo = self.copy_fixture("package-only")
        report = MODULE.DISCOVER_MODULE.build_report(repo)
        results = MODULE.build_results(report, apply=True, skip_init=True, command="shatter")

        target = results["targets"][0]
        self.assertEqual(target["status"], "integrated")
        self.assertEqual(target["selected_surface"]["type"], "package-json-scripts")
        package = json.loads((repo / "package.json").read_text(encoding="utf-8"))
        self.assertEqual(package["scripts"]["shatter"], "shatter scan .")

    def test_marks_multiple_local_surfaces_ambiguous_without_doc_preference(self) -> None:
        repo = self.copy_fixture("ambiguous-local")
        report = MODULE.DISCOVER_MODULE.build_report(repo)
        results = MODULE.build_results(report, apply=False, skip_init=True, command="shatter")

        target = results["targets"][0]
        self.assertEqual(target["status"], "ambiguous")
        self.assertEqual(len(target["proposals"]), 2)
        self.assertEqual(results["proposal_count"], 2)

        proposals = {proposal["surface"]["type"]: proposal for proposal in target["proposals"]}
        package_proposal = proposals["package-json-scripts"]
        self.assertEqual(package_proposal["wrapper_command"], "shatter scan .")
        self.assertEqual(package_proposal["edit"]["kind"], "package-json-script")
        self.assertEqual(package_proposal["edit"]["path"], "app/package.json")
        self.assertEqual(package_proposal["edit"]["script_value"], "shatter scan .")

        task_proposal = proposals["taskfile"]
        self.assertEqual(task_proposal["wrapper_command"], "shatter scan .")
        self.assertEqual(task_proposal["edit"]["kind"], "taskfile-task")
        self.assertEqual(task_proposal["edit"]["path"], "app/Taskfile.yml")
        self.assertEqual(task_proposal["edit"]["cmds"], ["shatter scan ."])

    def test_emits_proposals_for_ancestor_only_surfaces(self) -> None:
        repo = self.copy_fixture("ancestor-only")
        report = MODULE.DISCOVER_MODULE.build_report(repo)
        results = MODULE.build_results(report, apply=False, skip_init=True, command="shatter")

        target = results["targets"][0]
        self.assertEqual(target["status"], "ambiguous")
        self.assertEqual(target["reason"], "only ancestor surfaces available")
        self.assertEqual(len(target["proposals"]), 1)
        self.assertEqual(results["proposal_count"], 1)

        proposal = target["proposals"][0]
        self.assertEqual(proposal["surface"]["type"], "taskfile")
        self.assertEqual(proposal["surface"]["scope"], "ancestor")
        self.assertEqual(proposal["wrapper_command"], "shatter scan rust-lib")
        self.assertEqual(proposal["edit"]["kind"], "taskfile-task")
        self.assertEqual(proposal["edit"]["path"], "Taskfile.yml")
        self.assertEqual(proposal["edit"]["cmds"], ["shatter scan rust-lib"])

    def test_requires_confirmation_when_more_than_four_targets_detected(self) -> None:
        report = MODULE.DISCOVER_MODULE.build_report(
            Path(__file__).resolve().parent / "fixtures" / "over-four-repo"
        )
        results = {
            "repo_root": report["repo_root"],
            "target_count": report["target_count"],
            "requires_confirmation": True,
            "applied": False,
        }
        self.assertTrue(results["requires_confirmation"])

    def test_cli_apply_requires_confirmation_without_force(self) -> None:
        repo = Path(__file__).resolve().parent / "fixtures" / "over-four-repo"
        proc = subprocess.run(
            [
                sys.executable,
                str(SCRIPT_PATH),
                "--root",
                str(repo),
                "--apply",
                "--skip-init",
                "--json",
            ],
            check=True,
            capture_output=True,
            text=True,
        )
        payload = json.loads(proc.stdout)
        self.assertIn("confirmation required", payload["error"])
        self.assertFalse(payload["applied"])


if __name__ == "__main__":
    unittest.main()
