import json
import os
import tarfile
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts import package_npm_release


class PackageNpmReleaseTests(unittest.TestCase):
    def test_semver_from_continuous_tag(self):
        actual = package_npm_release.semver_from_tag("continuous-20260512-1735-abc123def456")
        self.assertEqual(actual, "0.0.0-continuous.20260512-1735-abc123def456")

    def test_wrapper_points_optional_dependencies_at_release_assets(self):
        with tempfile.TemporaryDirectory() as temp:
            out_dir = Path(temp)
            packages = [
                package_npm_release.PlatformPackage(
                    "linux-x86_64",
                    "@shatterproof/shatter-linux-x64",
                    "shatter-npm-linux-x64.tgz",
                    "shatter-linux-x86_64.tar.gz",
                )
            ]
            package_npm_release.write_wrapper_package(
                out_dir,
                "0.0.0-continuous.test",
                "shatterproof-ai/shatter",
                "continuous-test",
                packages,
            )

            with tarfile.open(out_dir / package_npm_release.WRAPPER_TARBALL, "r:gz") as tar:
                manifest = json.load(tar.extractfile("package/package.json"))

            self.assertEqual(manifest["name"], "@shatterproof/shatter")
            self.assertEqual(
                manifest["optionalDependencies"]["@shatterproof/shatter-linux-x64"],
                "https://github.com/shatterproof-ai/shatter/releases/download/continuous-test/shatter-npm-linux-x64.tgz",
            )

    def test_main_generates_wrapper_and_platform_tarballs(self):
        with tempfile.TemporaryDirectory() as temp:
            artifacts_dir = Path(temp) / "artifacts"
            artifacts_dir.mkdir()
            assets = []
            for platform, (_package_name, _tarball_name) in package_npm_release.SUPPORTED_PLATFORMS.items():
                asset_name = {
                    "linux-x86_64": "shatter-linux-x86_64.tar.gz",
                    "linux-aarch64": "shatter-linux-aarch64.tar.gz",
                    "darwin-x86_64": "shatter-macos-x86_64.tar.gz",
                    "darwin-aarch64": "shatter-macos-aarch64.tar.gz",
                }[platform]
                assets.append({"platform": platform, "name": asset_name})
                with tarfile.open(artifacts_dir / asset_name, "w:gz") as release_tar:
                    binary = Path(temp) / "shatter"
                    binary.write_text("#!/bin/sh\n")
                    release_tar.add(binary, arcname="shatter")

            (artifacts_dir / "shatter-release.json").write_text(
                json.dumps({"tag": "continuous-20260512-1735-abc123def456", "assets": assets})
            )

            with mock.patch.object(
                package_npm_release,
                "download_file",
                create=True,
            ):
                with mock.patch.dict(
                    os.environ,
                    {"GITHUB_REPOSITORY": "shatterproof-ai/shatter"},
                ):
                    exit_code = package_npm_release.main_with_args(
                        ["--artifacts-dir", str(artifacts_dir), "--out-dir", str(artifacts_dir)]
                    )

            self.assertEqual(exit_code, 0)
            self.assertTrue((artifacts_dir / package_npm_release.WRAPPER_TARBALL).is_file())
            self.assertTrue((artifacts_dir / "shatter-npm-linux-x64.tgz").is_file())


if __name__ == "__main__":
    unittest.main()
