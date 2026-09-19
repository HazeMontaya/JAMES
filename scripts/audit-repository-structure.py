#!/usr/bin/env python3
"""Structural and dependency audit for the canonical JAMES repository."""

from __future__ import annotations

import hashlib
import sys
from collections import defaultdict
from pathlib import Path
import tomllib

ROOT = Path(__file__).resolve().parents[1]
IGNORED = {".git", ".james", "out", "node_modules", "dist", "coverage", "__pycache__"}
FORBIDDEN_ROOTS = {"packages", "james"}
FORBIDDEN_FILES = {"pyproject.toml"}
CANONICAL_SINGLETONS = {
    "james_runtime.proto": "runtime/core/crates/james-runtime-client/proto/james_runtime.proto",
}


def files() -> list[Path]:
    return [
        p for p in ROOT.rglob("*")
        if p.is_file() and not any(part in IGNORED for part in p.relative_to(ROOT).parts)
    ]


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def cargo_manifests() -> list[tuple[Path, dict]]:
    result = []
    for path in ROOT.rglob("Cargo.toml"):
        if any(part in IGNORED for part in path.relative_to(ROOT).parts):
            continue
        with path.open("rb") as f:
            result.append((path, tomllib.load(f)))
    return result


def dependency_names(table: dict) -> set[str]:
    return set(table or {})


def main() -> int:
    errors: list[str] = []
    warnings: list[str] = []

    for name in FORBIDDEN_ROOTS:
        if (ROOT / name).exists():
            errors.append(f"retired root directory exists: {name}/")
    for name in FORBIDDEN_FILES:
        if (ROOT / name).exists():
            errors.append(f"retired root file exists: {name}")

    by_hash: dict[str, list[Path]] = defaultdict(list)
    for path in files():
        by_hash[digest(path)].append(path)
    for paths in by_hash.values():
        if len(paths) > 1:
            rel = sorted(str(p.relative_to(ROOT)).replace("\\", "/") for p in paths)
            errors.append("exact duplicate files: " + " <-> ".join(rel))

    for basename, canonical in CANONICAL_SINGLETONS.items():
        matches = [p for p in files() if p.name == basename]
        canonical_path = ROOT / canonical
        if not canonical_path.exists():
            errors.append(f"canonical singleton missing: {canonical}")
        for match in matches:
            if match != canonical_path:
                errors.append(f"duplicate canonical artifact: {match.relative_to(ROOT)}")

    manifests = cargo_manifests()
    package_defs: dict[str, Path] = {}
    workspaces: list[tuple[Path, dict, set[str]]] = []

    for path, data in manifests:
        package = data.get("package")
        if package:
            name = package.get("name")
            if name in package_defs:
                errors.append(
                    f"duplicate Cargo package name {name!r}: "
                    f"{package_defs[name].relative_to(ROOT)} and {path.relative_to(ROOT)}"
                )
            else:
                package_defs[name] = path

        ws = data.get("workspace")
        if ws:
            members = set(ws.get("members", []))
            workspaces.append((path.parent, data, members))

    # Every workspace member must contain a Cargo.toml and every package must
    # belong to exactly one canonical workspace.
    canonical_packages: dict[Path, str] = {}
    for ws_root, _, members in workspaces:
        for member in members:
            member_path = (ws_root / member).resolve()
            manifest = member_path / "Cargo.toml"
            if not manifest.exists():
                errors.append(f"workspace member missing Cargo.toml: {manifest.relative_to(ROOT)}")
                continue
            canonical_packages[manifest] = ws_root.as_posix()

    for path, data in manifests:
        if data.get("package") and path not in canonical_packages:
            errors.append(f"orphan Cargo package outside a workspace: {path.relative_to(ROOT)}")

        deps = {}
        for section in ("dependencies", "dev-dependencies", "build-dependencies"):
            deps.update(data.get(section, {}) or {})
        wsdeps = (data.get("workspace", {}) or {}).get("dependencies", {}) or {}

        for name, spec in deps.items():
            if isinstance(spec, dict) and "workspace" in spec and spec["workspace"] is True:
                # Resolve against the nearest canonical workspace manifest.
                owner = next((r for r, _, ms in workspaces if path in canonical_packages and canonical_packages[path] == r.as_posix()), None)
                if owner is not None:
                    owner_data = next(d for r, d, _ in workspaces if r == owner)
                    if name not in (owner_data.get("workspace", {}).get("dependencies", {}) or {}):
                        errors.append(
                            f"{path.relative_to(ROOT)} uses workspace dependency {name!r} "
                            f"but it is not declared by {owner.relative_to(ROOT) / 'Cargo.toml'}"
                        )

            if isinstance(spec, dict) and "path" in spec:
                target = (path.parent / spec["path"]).resolve()
                target_manifest = target / "Cargo.toml"
                if not target_manifest.exists():
                    errors.append(
                        f"{path.relative_to(ROOT)} has missing path dependency {name!r}: "
                        f"{target.relative_to(ROOT)}"
                    )
                else:
                    with target_manifest.open("rb") as f:
                        target_data = tomllib.load(f)
                    target_name = (target_data.get("package") or {}).get("name")
                    expected = spec.get("package", name)
                    if target_name and expected != target_name:
                        errors.append(
                            f"{path.relative_to(ROOT)} path dependency {name!r} resolves to "
                            f"package {target_name!r}, expected {expected!r}"
                        )

    # Python package layout must have exactly one import root.
    pyproject = ROOT / "runtime/python/sidecar/pyproject.toml"
    if not pyproject.exists():
        errors.append("canonical Python sidecar pyproject.toml is missing")
    else:
        with pyproject.open("rb") as f:
            py = tomllib.load(f)
        include = ((py.get("tool", {}).get("setuptools", {}).get("packages", {}).get("find", {})).get("include", []))
        if include != ["james_runtime*"]:
            warnings.append("sidecar setuptools package discovery is not explicitly limited to james_runtime*")

    if errors:
        print("JAMES repository/dependency audit: FAILED")
        for e in errors:
            print(" -", e)
        for w in warnings:
            print(" !", w)
        return 1

    print(f"JAMES repository/dependency audit: OK ({len(files())} files, {len(manifests)} Cargo manifests)")
    for w in warnings:
        print(" !", w)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
