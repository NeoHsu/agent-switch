#!/usr/bin/env python3
"""Verify exact version lockstep between ags and the bundled agent Skill."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

SEMVER = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="repository root (defaults to this script's parent repository)",
    )
    parser.add_argument(
        "--tag",
        help="release tag to verify in addition to checked-in versions (for example v0.2.1)",
    )
    return parser.parse_args()


def read_utf8(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        raise ValueError(f"failed to read {path}: {error}") from error


def load_json(path: Path) -> dict[str, Any]:
    try:
        document = json.loads(read_utf8(path))
    except json.JSONDecodeError as error:
        raise ValueError(f"failed to parse {path}: {error}") from error
    if not isinstance(document, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return document


def section(text: str, name: str) -> str:
    match = re.search(
        rf"(?ms)^\[{re.escape(name)}\]\s*$\n(.*?)(?=^\[|\Z)",
        text,
    )
    if match is None:
        raise ValueError(f"Cargo.toml has no [{name}] section")
    return match.group(1)


def workspace_version(path: Path) -> str:
    match = re.search(r'^version\s*=\s*"([^"]+)"\s*$', section(read_utf8(path), "workspace.package"), re.MULTILINE)
    if match is None:
        raise ValueError(f"{path} has no workspace.package.version")
    return match.group(1)


def verify(repo: Path, release_tag: str | None) -> tuple[str, list[str]]:
    cli_version = workspace_version(repo / "Cargo.toml")
    manifest_path = repo / "skills/agent-switch/compatibility.json"
    manifest = load_json(manifest_path)
    skill_path = repo / "skills/agent-switch/SKILL.md"
    skill = read_utf8(skill_path)
    errors: list[str] = []

    if not SEMVER.fullmatch(cli_version):
        errors.append(f"CLI package version is not SemVer: {cli_version!r}")

    expected_tag = f"v{cli_version}"
    expected_versions = {
        "skillVersion": manifest.get("skillVersion"),
        "cliVersion": manifest.get("cliVersion"),
    }
    for label, version in expected_versions.items():
        if version != cli_version:
            errors.append(f"Skill manifest {label} {version!r} does not match CLI {cli_version!r}")

    if manifest.get("schemaVersion") != 1:
        errors.append("Skill compatibility schemaVersion must be 1")
    if manifest.get("skillName") != "agent-switch":
        errors.append("Skill compatibility skillName must be `agent-switch`")
    if manifest.get("compatibility") != "exact":
        errors.append("Skill compatibility policy must be `exact`")
    if manifest.get("releaseTag") != expected_tag:
        errors.append(
            f"Skill releaseTag {manifest.get('releaseTag')!r} does not match {expected_tag}"
        )

    compatibility_match = re.search(
        r"^compatibility:\s*Requires ags CLI ([^ ]+) exactly$", skill, re.MULTILINE
    )
    if compatibility_match is None:
        errors.append("SKILL.md frontmatter has no exact ags CLI compatibility declaration")
    elif compatibility_match.group(1) != cli_version:
        errors.append(
            "SKILL.md compatibility version "
            f"{compatibility_match.group(1)!r} does not match CLI {cli_version!r}"
        )

    gate = f"ags doctor --skill-version {cli_version} --json"
    if gate not in skill:
        errors.append(f"SKILL.md compatibility gate is missing `{gate}`")
    if gate not in manifest.get("requiredCommands", []):
        errors.append(f"compatibility manifest requiredCommands is missing `{gate}`")

    if release_tag is not None and release_tag != expected_tag:
        errors.append(f"release tag {release_tag!r} does not match {expected_tag!r}")

    return cli_version, errors


def main() -> int:
    args = parse_args()
    try:
        repo = args.repo.resolve(strict=True)
        version, errors = verify(repo, args.tag)
    except (OSError, ValueError) as error:
        sys.stderr.write(f"Skill version check failed: {error}\n")
        return 1

    if errors:
        for error in errors:
            sys.stderr.write(f"Skill version check failed: {error}\n")
        return 1

    sys.stdout.write(f"verified ags CLI and Skill exact lockstep at {version}\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
