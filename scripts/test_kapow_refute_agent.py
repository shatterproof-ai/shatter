from __future__ import annotations

import stat
import tempfile
import unittest
from pathlib import Path

from scripts import kapow_refute_agent


class KapowRefuteAgentTest(unittest.TestCase):
    def test_refute_binary_path_uses_project_local_agents_bin(self) -> None:
        project = Path("/workspace/kapow")

        self.assertEqual(
            kapow_refute_agent.refute_binary(project),
            project / ".agents" / "bin" / "refute",
        )

    def test_check_reports_missing_binary_with_install_command(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            project = Path(tmp) / "kapow"
            project.mkdir()
            refute_checkout = Path("/opt/refute")

            result = kapow_refute_agent.check_install(project, refute_checkout)

        self.assertEqual(result.exit_code, 2)
        self.assertIn("missing", result.status)
        self.assertIn("bash /opt/refute/scripts/install-nightly.sh --project", result.message)

    def test_check_accepts_executable_binary(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            project = Path(tmp) / "kapow"
            bin_dir = project / ".agents" / "bin"
            bin_dir.mkdir(parents=True)
            refute = bin_dir / "refute"
            refute.write_text("#!/usr/bin/env bash\n", encoding="utf-8")
            refute.chmod(refute.stat().st_mode | stat.S_IEXEC)

            result = kapow_refute_agent.check_install(project, Path("/opt/refute"))

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.status, "ok")
        self.assertIn(".agents/bin/refute", result.message)


if __name__ == "__main__":
    unittest.main()
