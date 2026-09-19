"""Transactional Git checkpoints for guarded JAMES self-modification.

The manager deliberately does not push, reset --hard, or execute arbitrary commands.
It provides a narrow transaction boundary: checkpoint -> inspect -> verify -> commit,
or rollback the working tree to the checkpoint when verification fails.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import os
import subprocess
from typing import Sequence


@dataclass(frozen=True)
class Checkpoint:
    commit: str
    branch: str
    clean: bool


@dataclass(frozen=True)
class VerificationResult:
    success: bool
    command: tuple[str, ...]
    returncode: int
    stdout: str
    stderr: str


class GitCheckpointManager:
    """Bounded Git transaction manager for a single workspace."""

    def __init__(self, root: str | Path, *, timeout_seconds: float = 30.0) -> None:
        self.root = Path(root).expanduser().resolve()
        self.timeout_seconds = timeout_seconds
        self._validate_repo()

    def _validate_repo(self) -> None:
        if not self.root.is_dir():
            raise ValueError(f"workspace does not exist: {self.root}")
        probe = self._git("rev-parse", "--show-toplevel")
        actual = Path(probe.stdout.strip()).resolve()
        if actual != self.root:
            raise ValueError("workspace must be the Git repository root")

    def _git(self, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
        return self._run(("git", *args), check=check)

    def _run(
        self, command: Sequence[str], *, check: bool = True
    ) -> subprocess.CompletedProcess[str]:
        env = {
            "PATH": os.environ.get("PATH", ""),
            "SYSTEMROOT": os.environ.get("SYSTEMROOT", ""),
            "WINDIR": os.environ.get("WINDIR", ""),
            "HOME": os.environ.get("HOME", ""),
            "USERPROFILE": os.environ.get("USERPROFILE", ""),
            "TEMP": os.environ.get("TEMP", ""),
            "TMP": os.environ.get("TMP", ""),
        }
        result = subprocess.run(
            list(command),
            cwd=self.root,
            env=env,
            text=True,
            capture_output=True,
            timeout=self.timeout_seconds,
            check=False,
        )
        if check and result.returncode != 0:
            raise RuntimeError(
                f"command failed ({result.returncode}): {' '.join(command)}\n"
                f"{result.stderr.strip()}"
            )
        return result

    def status(self) -> str:
        return self._git("status", "--short").stdout

    def diff(self) -> str:
        return self._git("diff", "--no-ext-diff", "--").stdout

    def create_checkpoint(self, message: str = "james: self-improvement checkpoint") -> Checkpoint:
        if not message.strip():
            raise ValueError("checkpoint message must not be empty")
        status = self.status()
        if status:
            raise RuntimeError(
                "working tree must be clean before creating a checkpoint; "
                "self-improvement changes must start from a known baseline"
            )
        branch = self._git("branch", "--show-current").stdout.strip()
        if not branch:
            raise RuntimeError("detached HEAD is not supported for self-improvement")
        commit = self._git("rev-parse", "HEAD").stdout.strip()
        return Checkpoint(commit=commit, branch=branch, clean=True)

    def verify(
        self,
        command: Sequence[str],
        *,
        timeout_seconds: float | None = None,
    ) -> VerificationResult:
        if not command or not command[0].strip():
            raise ValueError("verification command must not be empty")
        old_timeout = self.timeout_seconds
        if timeout_seconds is not None:
            self.timeout_seconds = timeout_seconds
        try:
            result = self._run(tuple(command), check=False)
        finally:
            self.timeout_seconds = old_timeout
        return VerificationResult(
            success=result.returncode == 0,
            command=tuple(command),
            returncode=result.returncode,
            stdout=result.stdout,
            stderr=result.stderr,
        )

    def commit_verified(
        self,
        checkpoint: Checkpoint,
        *,
        message: str,
        verification: VerificationResult,
    ) -> str:
        self._assert_checkpoint(checkpoint)
        if not verification.success:
            raise RuntimeError("refusing to commit: verification failed")
        if not self.status():
            raise RuntimeError("refusing to commit: no changes detected")
        if not message.strip():
            raise ValueError("commit message must not be empty")
        self._git("diff", "--check")
        self._git("add", "--all")
        self._git("commit", "-m", message)
        return self._git("rev-parse", "HEAD").stdout.strip()

    def rollback(self, checkpoint: Checkpoint) -> None:
        self._assert_checkpoint(checkpoint)
        # Restore tracked files and remove untracked files/directories created
        # by the candidate change. This is intentionally scoped to this repo.
        self._git("reset", "--hard", checkpoint.commit)
        self._git("clean", "-fd")

    def _assert_checkpoint(self, checkpoint: Checkpoint) -> None:
        current_branch = self._git("branch", "--show-current").stdout.strip()
        if current_branch != checkpoint.branch:
            raise RuntimeError("checkpoint branch no longer matches current branch")
        reachable = self._git(
            "merge-base", "--is-ancestor", checkpoint.commit, "HEAD", check=False
        )
        if reachable.returncode != 0:
            raise RuntimeError("checkpoint is not an ancestor of the current HEAD")
