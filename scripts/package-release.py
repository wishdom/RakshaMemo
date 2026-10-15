#!/usr/bin/env python3
"""Package a built RakshaMemo binary; requires Python 3.11 or newer.

Example: python scripts/package-release.py package --target x86_64-pc-windows-msvc
macOS packaging must run on macOS (it uses codesign and ditto).
"""

import argparse
import hashlib
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib


ROOT = Path(__file__).resolve().parent.parent
ASSETS = {
    "x86_64-pc-windows-msvc": "RakshaMemo.exe",
    "aarch64-apple-darwin": "RakshaMemo-macOS-Apple-Silicon.zip",
    "x86_64-apple-darwin": "RakshaMemo-macOS-Intel.zip",
    "x86_64-unknown-linux-gnu": "RakshaMemo-Linux-x86_64.tar.gz",
}


def version_for(tag):
    with (ROOT / "Cargo.toml").open("rb") as manifest:
        version = tomllib.load(manifest)["package"]["version"]
    if tag is not None and tag != f"v{version}":
        raise ValueError(f"Tag {tag!r} must match Cargo.toml version: v{version}")
    return version


def package(args):
    version = version_for(args.tag)
    windows = args.target.endswith("windows-msvc")
    binary_name = "RakshaMemo.exe" if windows else "RakshaMemo"
    binary = args.binary or ROOT / "target" / args.target / "release" / binary_name
    if not binary.is_file() or binary.stat().st_size == 0:
        raise ValueError(f"Built application is missing or empty: {binary}")
    args.output.mkdir(parents=True, exist_ok=True)
    destination = args.output.resolve() / ASSETS[args.target]
    if windows:
        shutil.copy2(binary, destination)
    elif args.target.endswith("linux-gnu"):
        with tarfile.open(destination, "w:gz") as archive:
            entry = archive.gettarinfo(binary, arcname="RakshaMemo")
            entry.mode = 0o755
            entry.uid = entry.gid = 0
            entry.uname = entry.gname = ""
            with binary.open("rb") as source:
                archive.addfile(entry, source)
    else:
        if sys.platform != "darwin":
            raise ValueError("macOS app bundles must be packaged on macOS")
        with tempfile.TemporaryDirectory(prefix="rakshamemo-release-") as staging:
            app = Path(staging) / "RakshaMemo.app"
            executable = app / "Contents" / "MacOS" / "RakshaMemo"
            executable.parent.mkdir(parents=True)
            shutil.copy2(binary, executable)
            executable.chmod(0o755)
            with (app / "Contents" / "Info.plist").open("wb") as info:
                plistlib.dump({
                    "CFBundleName": "RakshaMemo",
                    "CFBundleDisplayName": "RakshaMemo",
                    "CFBundleIdentifier": "io.github.wishdom.rakshamemo",
                    "CFBundleExecutable": "RakshaMemo",
                    "CFBundlePackageType": "APPL",
                    "CFBundleShortVersionString": version,
                    "CFBundleVersion": version.split("-")[0].split("+")[0],
                    "LSMinimumSystemVersion": os.environ.get("MACOSX_DEPLOYMENT_TARGET", "13.0"),
                    "NSHighResolutionCapable": True,
                }, info)
            # Ad-hoc signing supports local execution; it is not Apple notarization.
            subprocess.run(["codesign", "--force", "--sign", "-", str(app)], check=True)
            subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
            subprocess.run([
                "ditto", "-c", "-k", "--sequesterRsrc", "--keepParent",
                str(app), str(destination),
            ], check=True)
    print(destination)


def checksums(output):
    expected = set(ASSETS.values())
    actual = {path.name for path in output.iterdir() if path.name != "SHA256SUMS.txt"}
    if actual != expected:
        raise ValueError(f"Expected four release assets; missing={expected - actual}, extra={actual - expected}")
    lines = []
    for name in sorted(expected):
        with (output / name).open("rb") as asset:
            digest = hashlib.file_digest(asset, "sha256").hexdigest()
        lines.append(f"{digest}  {name}\n")
    destination = output / "SHA256SUMS.txt"
    destination.write_text("".join(lines), encoding="utf-8", newline="\n")
    print(destination)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    check = commands.add_parser("check-version", help="Check that a release tag matches Cargo.toml")
    check.add_argument("--tag", required=True)
    build = commands.add_parser("package", help="Package an already-built release binary")
    build.add_argument("--target", choices=ASSETS, required=True)
    build.add_argument("--binary", type=Path, help="Override target/<target>/release/RakshaMemo[.exe]")
    build.add_argument("--tag", help="Require this tag to match Cargo.toml")
    sums = commands.add_parser("checksums", help="Verify all four assets exist and write SHA256SUMS.txt")
    for command in (build, sums):
        command.add_argument("--output", type=Path, default=ROOT / "target" / "release-assets")
    args = parser.parse_args()
    try:
        if args.command == "check-version":
            print(version_for(args.tag))
        elif args.command == "package":
            package(args)
        else:
            checksums(args.output)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Error: {error}\n")


if __name__ == "__main__":
    main()
