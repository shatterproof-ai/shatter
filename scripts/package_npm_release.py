#!/usr/bin/env python3
"""Build registryless npm tarballs from Shatter release artifacts."""

from __future__ import annotations

import argparse
import io
import json
import os
import stat
import tarfile
import tempfile
from dataclasses import dataclass
from pathlib import Path


SUPPORTED_PLATFORMS = {
    "linux-x86_64": ("@shatterproof/shatter-linux-x64", "shatter-npm-linux-x64.tgz"),
    "linux-aarch64": ("@shatterproof/shatter-linux-arm64", "shatter-npm-linux-arm64.tgz"),
    "darwin-x86_64": ("@shatterproof/shatter-darwin-x64", "shatter-npm-darwin-x64.tgz"),
    "darwin-aarch64": ("@shatterproof/shatter-darwin-arm64", "shatter-npm-darwin-arm64.tgz"),
}


WRAPPER_PACKAGE = "@shatterproof/shatter"
WRAPPER_TARBALL = "shatter-npm-wrapper.tgz"


@dataclass(frozen=True)
class PlatformPackage:
    platform: str
    package_name: str
    tarball_name: str
    asset_name: str


def semver_from_tag(tag: str) -> str:
    if not tag.startswith("continuous-"):
        raise ValueError(f"expected continuous release tag, got {tag!r}")
    suffix = tag.removeprefix("continuous-").replace("_", "-")
    safe = "".join(ch if ch.isalnum() or ch in ".-" else "-" for ch in suffix)
    return f"0.0.0-continuous.{safe}"


def release_url(repo: str, tag: str, asset_name: str) -> str:
    return f"https://github.com/{repo}/releases/download/{tag}/{asset_name}"


def package_json_bytes(package: dict[str, object]) -> bytes:
    return (json.dumps(package, indent=2, sort_keys=True) + "\n").encode()


def add_bytes(tar: tarfile.TarFile, path: str, content: bytes, mode: int = 0o644) -> None:
    info = tarfile.TarInfo(f"package/{path}")
    info.size = len(content)
    info.mode = mode
    tar.addfile(info, io.BytesIO(content))


def add_path(tar: tarfile.TarFile, source: Path, package_path: str) -> None:
    info = tar.gettarinfo(str(source), f"package/{package_path}")
    if source.name == "shatter":
        info.mode |= stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH
    with source.open("rb") as handle:
        tar.addfile(info, handle)


def write_platform_package(
    artifacts_dir: Path,
    out_dir: Path,
    version: str,
    package: PlatformPackage,
) -> None:
    archive = artifacts_dir / package.asset_name
    if not archive.is_file():
        raise FileNotFoundError(f"missing release archive: {archive}")

    package_manifest = {
        "name": package.package_name,
        "version": version,
        "description": f"Shatter binary payload for {package.platform}",
        "license": "UNLICENSED",
        "os": ["linux" if package.platform.startswith("linux") else "darwin"],
        "cpu": ["x64" if package.platform.endswith("x86_64") else "arm64"],
        "bin": {"shatter": "./shatter"},
        "files": ["shatter", "shatter-rust", "shatter-go", "shatter-ts"],
    }

    with tempfile.TemporaryDirectory() as temp_dir:
        extract_dir = Path(temp_dir)
        with tarfile.open(archive, "r:gz") as release_tar:
            release_tar.extractall(extract_dir, filter="data")

        with tarfile.open(out_dir / package.tarball_name, "w:gz") as npm_tar:
            add_bytes(npm_tar, "package.json", package_json_bytes(package_manifest))
            for source in sorted(extract_dir.iterdir()):
                if source.is_file():
                    add_path(npm_tar, source, source.name)
                elif source.is_dir():
                    for child in sorted(source.rglob("*")):
                        if child.is_file():
                            add_path(npm_tar, child, str(child.relative_to(extract_dir)))


def wrapper_bin_script() -> bytes:
    return b"""#!/usr/bin/env node
const { spawn } = require("node:child_process");
const path = require("node:path");

const packages = {
  "linux-x64": "@shatterproof/shatter-linux-x64",
  "linux-arm64": "@shatterproof/shatter-linux-arm64",
  "darwin-x64": "@shatterproof/shatter-darwin-x64",
  "darwin-arm64": "@shatterproof/shatter-darwin-arm64"
};

const key = `${process.platform}-${process.arch}`;
const packageName = packages[key];

if (!packageName) {
  console.error(`Unsupported Shatter platform: ${key}`);
  process.exit(1);
}

let binary = process.env.SHATTER_BINARY;
if (!binary) {
  try {
    const manifest = require.resolve(`${packageName}/package.json`);
    binary = path.join(path.dirname(manifest), "shatter");
  } catch (error) {
    console.error(`Shatter platform package ${packageName} is not installed.`);
    console.error("Reinstall @shatterproof/shatter from the GitHub Release tarball for this build.");
    process.exit(1);
  }
}

const child = spawn(binary, process.argv.slice(2), { stdio: "inherit" });
child.on("error", (error) => {
  console.error(error.message);
  process.exit(1);
});
child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
"""


def write_wrapper_package(
    out_dir: Path,
    version: str,
    repo: str,
    tag: str,
    platform_packages: list[PlatformPackage],
) -> None:
    optional_dependencies = {
        package.package_name: release_url(repo, tag, package.tarball_name)
        for package in platform_packages
    }
    package_manifest = {
        "name": WRAPPER_PACKAGE,
        "version": version,
        "description": "Registryless Shatter CLI wrapper for GitHub Release binary payloads",
        "license": "UNLICENSED",
        "bin": {"shatter": "./bin/shatter.js"},
        "optionalDependencies": optional_dependencies,
    }

    with tarfile.open(out_dir / WRAPPER_TARBALL, "w:gz") as npm_tar:
        add_bytes(npm_tar, "package.json", package_json_bytes(package_manifest))
        add_bytes(npm_tar, "bin/shatter.js", wrapper_bin_script(), mode=0o755)


def platform_packages(manifest: dict[str, object]) -> list[PlatformPackage]:
    packages = []
    assets = manifest.get("assets", [])
    if not isinstance(assets, list):
        raise ValueError("manifest assets must be a list")
    for asset in assets:
        if not isinstance(asset, dict):
            continue
        platform = asset.get("platform")
        name = asset.get("name")
        if not isinstance(platform, str) or not isinstance(name, str):
            continue
        package_info = SUPPORTED_PLATFORMS.get(platform)
        if package_info is None:
            continue
        package_name, tarball_name = package_info
        packages.append(PlatformPackage(platform, package_name, tarball_name, name))
    return packages


def main_with_args(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts-dir", type=Path, default=Path("artifacts"))
    parser.add_argument("--out-dir", type=Path, default=None)
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", "shatterproof-ai/shatter"))
    args = parser.parse_args(argv)

    artifacts_dir = args.artifacts_dir
    out_dir = args.out_dir or artifacts_dir
    out_dir.mkdir(parents=True, exist_ok=True)

    manifest = json.loads((artifacts_dir / "shatter-release.json").read_text())
    tag = manifest["tag"]
    version = semver_from_tag(tag)
    packages = platform_packages(manifest)
    if len(packages) != len(SUPPORTED_PLATFORMS):
        found = ", ".join(package.platform for package in packages)
        raise SystemExit(f"manifest did not contain all supported npm platforms; found: {found}")

    for package in packages:
        write_platform_package(artifacts_dir, out_dir, version, package)
    write_wrapper_package(out_dir, version, args.repo, tag, packages)

    asset_names = [package.tarball_name for package in packages] + [WRAPPER_TARBALL]
    print("\n".join(asset_names))
    return 0


def main() -> int:
    return main_with_args()


if __name__ == "__main__":
    raise SystemExit(main())
