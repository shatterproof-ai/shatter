import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
RELEASE_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "release.yml"


class ReleaseWorkflowTests(unittest.TestCase):
    def test_continuous_release_is_prerelease(self):
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")

        self.assertIn("gh release create", workflow)
        self.assertIn("--prerelease", workflow)


if __name__ == "__main__":
    unittest.main()
