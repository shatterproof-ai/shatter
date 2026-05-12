import json
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
INSTALLER = REPO_ROOT / "install.sh"


def run_installer_snippet(snippet: str) -> str:
    command = f"""
set -euo pipefail
SHATTER_INSTALLER_NO_MAIN=1 source "{INSTALLER}"
{snippet}
"""
    result = subprocess.run(
        ["bash", "-c", command],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return result.stdout.strip()


class InstallManifestTests(unittest.TestCase):
    def test_archive_name_matches_release_asset_names(self):
        cases = {
            "linux-x86_64": "shatter-linux-x86_64.tar.gz",
            "linux-aarch64": "shatter-linux-aarch64.tar.gz",
            "darwin-x86_64": "shatter-macos-x86_64.tar.gz",
            "darwin-aarch64": "shatter-macos-aarch64.tar.gz",
        }
        for platform, expected in cases.items():
            with self.subTest(platform=platform):
                actual = run_installer_snippet(
                    f'PLATFORM="{platform}"\narchive_name_for_platform'
                )
                self.assertEqual(actual, expected)

    def test_manifest_field_selects_platform_asset(self):
        manifest = {
            "assets": [
                {
                    "platform": "linux-aarch64",
                    "name": "shatter-linux-aarch64.tar.gz",
                    "url": "https://example.invalid/linux-arm64",
                    "sha256": "aaa",
                },
                {
                    "platform": "darwin-aarch64",
                    "name": "shatter-macos-aarch64.tar.gz",
                    "url": "https://example.invalid/macos-arm64",
                    "sha256": "bbb",
                },
            ]
        }
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as handle:
            json.dump(manifest, handle)
            manifest_path = handle.name

        try:
            actual = run_installer_snippet(
                f'manifest_field "{manifest_path}" "darwin-aarch64" "sha256"'
            )
            self.assertEqual(actual, "bbb")
        finally:
            Path(manifest_path).unlink(missing_ok=True)

    def test_version_alias_sets_build_without_latest_lookup(self):
        actual = run_installer_snippet(
            'unset BUILD\nVERSION="continuous-test"\nresolve_build\nprintf "%s" "$BUILD"'
        )
        self.assertEqual(actual.splitlines()[-1], "continuous-test")


if __name__ == "__main__":
    unittest.main()
