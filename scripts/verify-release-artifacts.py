#!/usr/bin/env python3
"""Verify Agent Switch release archives and their checksum manifest."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import platform
import re
import stat
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path
from typing import NoReturn

ARCHIVES = {
    "ags-linux-x86_64.tar.gz": "ags",
    "ags-macos-aarch64.tar.gz": "ags",
    "ags-macos-x86_64.tar.gz": "ags",
    "ags-windows-x86_64.zip": "ags.exe",
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, default=Path.cwd())
    parser.add_argument("--version", help="release version to include in the verification report")
    parser.add_argument(
        "--version-from-cargo",
        action="store_true",
        help="read the expected version from Cargo.toml in the repository",
    )
    parser.add_argument(
        "--execute-native",
        action="store_true",
        help="execute the archive compatible with the current host and verify ags version --json",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="create synthetic archives and verify the checker itself",
    )
    return parser.parse_args()


def fail(message: str) -> NoReturn:
    raise ValueError(message)


def checked_archive_path(root: Path, name: str) -> Path:
    if name not in ARCHIVES:
        fail(f"unknown release archive: {name}")
    relative = Path(name)
    if relative.is_absolute() or relative.name != name or ".." in relative.parts:
        fail(f"unsafe release archive path: {name}")
    return root / relative


def checked_member_name(name: str) -> str:
    if name not in {"ags", "ags.exe"}:
        fail(f"unsafe archive member path: {name}")
    relative = Path(name)
    if relative.is_absolute() or relative.name != name or ".." in relative.parts:
        fail(f"unsafe archive member path: {name}")
    return name


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def cargo_version(repo: Path) -> str:
    text = (repo / "Cargo.toml").read_text(encoding="utf-8")
    section = re.search(r"(?ms)^\[workspace\.package\]\s*$\n(.*?)(?=^\[|\Z)", text)
    if section is None:
        fail("Cargo.toml has no [workspace.package] section")
    version = re.search(r'^version\s*=\s*"([^"]+)"\s*$', section.group(1), re.MULTILINE)
    if version is None:
        fail("Cargo.toml has no workspace package version")
    return version.group(1)


def checksum_manifest(root: Path) -> None:
    manifest = root / "SHA256SUMS"
    if not manifest.is_file():
        fail("SHA256SUMS is missing")

    expected: dict[str, str] = {}
    for line_number, line in enumerate(manifest.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        fields = line.split(maxsplit=1)
        if len(fields) != 2 or not re.fullmatch(r"[0-9a-fA-F]{64}", fields[0]):
            fail(f"invalid SHA256SUMS entry at line {line_number}")
        expected[fields[1].lstrip("*")] = fields[0].lower()

    if set(expected) != set(ARCHIVES):
        fail(
            "SHA256SUMS entries do not match archives: "
            f"expected {sorted(ARCHIVES)}, found {sorted(expected)}"
        )
    for name in ARCHIVES:
        path = checked_archive_path(root, name)
        if not path.is_file():
            fail(f"release archive is missing: {name}")
        actual = sha256(path)
        if actual != expected[name]:
            fail(f"checksum mismatch for {name}: expected {expected[name]}, got {actual}")


def verify_tar(path: Path, expected_member: str) -> None:
    expected_member = checked_member_name(expected_member)
    try:
        with tarfile.open(path, "r:gz") as archive:
            members = archive.getmembers()
            if [member.name for member in members] != [expected_member]:
                fail(f"{path.name} must contain only {expected_member}")
            member = members[0]
            if not member.isfile():
                fail(f"{path.name} member {expected_member} is not a regular file")
            if not member.mode & stat.S_IXUSR:
                fail(f"{path.name} member {expected_member} is not executable")
            if member.size == 0:
                fail(f"{path.name} member {expected_member} is empty")
    except tarfile.TarError as error:
        fail(f"invalid tar archive {path.name}: {error}")


def verify_zip(path: Path, expected_member: str) -> None:
    expected_member = checked_member_name(expected_member)
    try:
        with zipfile.ZipFile(path) as archive:
            names = archive.namelist()
            if names != [expected_member]:
                fail(f"{path.name} must contain only {expected_member}")
            info = archive.infolist()[0]
            if info.is_dir() or info.file_size == 0:
                fail(f"{path.name} member {expected_member} must be a non-empty file")
            if ".." in Path(info.filename).parts or Path(info.filename).is_absolute():
                fail(f"unsafe path in {path.name}: {info.filename}")
    except (zipfile.BadZipFile, IndexError) as error:
        fail(f"invalid zip archive {path.name}: {error}")


def verify_archives(root: Path, version: str | None, execute_native: bool = False) -> None:
    checksum_manifest(root)
    for name, member in ARCHIVES.items():
        path = checked_archive_path(root, name)
        if path.suffix == ".zip":
            verify_zip(path, member)
        else:
            verify_tar(path, member)

    if execute_native:
        execute_native_archive(root, version)

    label = version or "unknown"
    print(f"verified Agent Switch release archives and checksums for {label}")


def native_archive_name() -> str | None:
    system = platform.system()
    machine = platform.machine().lower()
    if system == "Linux" and machine in {"x86_64", "amd64"}:
        return "ags-linux-x86_64.tar.gz"
    if system == "Darwin" and machine in {"arm64", "aarch64"}:
        return "ags-macos-aarch64.tar.gz"
    if system == "Darwin" and machine in {"x86_64", "amd64"}:
        return "ags-macos-x86_64.tar.gz"
    if system == "Windows" and machine in {"x86_64", "amd64"}:
        return "ags-windows-x86_64.zip"
    return None


def execute_native_archive(root: Path, version: str | None) -> None:
    archive_name = native_archive_name()
    if archive_name is None:
        print("skipped native archive execution: unsupported host")
        return

    archive_path = checked_archive_path(root, archive_name)
    expected_member = checked_member_name(ARCHIVES[archive_name])
    with tempfile.TemporaryDirectory(prefix="ags-release-verify-") as directory:
        extracted = Path(directory) / expected_member
        if archive_name.endswith(".zip"):
            with zipfile.ZipFile(archive_path) as archive:
                extracted.write_bytes(archive.read(expected_member))
        else:
            with tarfile.open(archive_path, "r:gz") as archive:
                data = archive.extractfile(expected_member)
                if data is None:
                    fail(f"could not extract {expected_member} from {archive_name}")
                extracted.write_bytes(data.read())
            extracted.chmod(extracted.stat().st_mode | stat.S_IXUSR)

        result = subprocess.run(
            [str(extracted), "version", "--json"],
            check=False,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            fail(f"native archive execution failed for {archive_name}: {result.stderr.strip()}")
        try:
            report: object = json.loads(result.stdout)
        except json.JSONDecodeError as error:
            fail(f"native archive returned invalid JSON for {archive_name}: {error}")
        if not isinstance(report, dict):
            fail(f"native archive returned a non-object JSON response for {archive_name}")
        if version is not None and report.get("version") != version:
            fail(
                f"native archive version mismatch for {archive_name}: "
                f"expected {version}, got {report.get('version')!r}"
            )


def write_tar(path: Path, member_name: str) -> None:
    payload = b"#!/bin/sh\necho synthetic\n"
    info = tarfile.TarInfo(member_name)
    info.mode = 0o755
    info.size = len(payload)
    with tarfile.open(path, "w:gz") as archive:
        archive.addfile(info, io.BytesIO(payload))


def write_zip(path: Path, member_name: str) -> None:
    info = zipfile.ZipInfo(member_name)
    info.external_attr = (0o755 & 0xFFFF) << 16
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr(info, b"synthetic executable\n")


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="ags-release-check-") as directory:
        root = Path(directory)
        for name, member in ARCHIVES.items():
            if name.endswith(".zip"):
                write_zip(root / name, member)
            else:
                write_tar(root / name, member)
        lines = [f"{sha256(root / name)}  {name}" for name in ARCHIVES]
        (root / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")
        verify_archives(root, "self-test")
    print("release artifact verifier self-test passed")


def main() -> int:
    args = parse_args()
    try:
        if args.self_test:
            self_test()
            return 0
        version = args.version
        if args.version_from_cargo:
            version = cargo_version(args.directory)
        verify_archives(args.directory.resolve(), version, args.execute_native)
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        sys.stderr.write(f"release artifact verification failed: {error}\n")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
