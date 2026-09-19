"""Pytest environment normalization for Windows developer shells.

Some Visual Studio developer environments rewrite LOCALAPPDATA/TEMP into a
nonexistent path. Pytest's built-in tmp_path fixture then fails before tests
start. Prefer the user's normal per-user temp directory when it is writable.
"""
from __future__ import annotations

import os
import tempfile
from pathlib import Path


def _configure_windows_temp() -> None:
    if os.name != "nt":
        return

    candidates = [
        Path(os.environ.get("USERPROFILE", "")) / "AppData" / "Local" / "Temp",
        Path.home() / "AppData" / "Local" / "Temp",
    ]

    for candidate in candidates:
        try:
            candidate = candidate.resolve()
            candidate.mkdir(parents=True, exist_ok=True)
            probe = candidate / ".james-pytest-write-probe"
            probe.write_text("ok", encoding="utf-8")
            probe.unlink()
        except (OSError, RuntimeError):
            continue

        os.environ["TEMP"] = str(candidate)
        os.environ["TMP"] = str(candidate)
        tempfile.tempdir = str(candidate)
        break


_configure_windows_temp()
