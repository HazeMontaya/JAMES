#!/usr/bin/env python3
"""Validate JAMES repository layout and detect accidental duplicate source files."""

from __future__ import annotations

import hashlib
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
IGNORED_DIRS = {".git", ".james", "out", "node_modules", "dist", "coverage"}
FORBIDDEN_ROOTS = {"packages", "james"}
FORBIDDEN_FILES = {"pyproject.toml"}


def digest(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def iter_files() -> list[Path]:
    files: list[Path] = []
    for path in ROOT.rglob("*"):
        if not path.is_file():
            continue
        rel = path.relative_to(ROOT)
        if any(part in IGNORED_DIRS for part in rel.parts):
            continue
        files.append(path)
    return files


def main() -> int:
    errors: list[str] = []
    by_hash: dict[str, list[Path]] = defaultdict(list)

    for path in iter_files():
        by_hash[digest(path)].append(path)

    for paths in by_hash.values():
        if len(paths) > 1:
            rel = [str(p.relative_to(ROOT)).replace("\\", "/") for p in paths]
            errors.append("exact duplicate files: " + " <-> ".join(sorted(rel)))

    for name in FORBIDDEN_ROOTS:
        if (ROOT / name).exists():
            errors.append(f"retired root directory exists: {name}/")

    for name in FORBIDDEN_FILES:
        if (ROOT / name).exists():
            errors.append(f"retired root file exists: {name}")

    if errors:
        print("JAMES structure audit: FAILED")
        for error in errors:
            print(f" - {error}")
        return 1

    print(f"JAMES structure audit: OK ({len(iter_files())} tracked candidate files scanned)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
