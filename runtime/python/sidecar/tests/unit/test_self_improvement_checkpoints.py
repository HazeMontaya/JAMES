from pathlib import Path
import subprocess

import pytest

from james_runtime.self_improvement import GitCheckpointManager


def _git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=root,
        text=True,
        capture_output=True,
        check=True,
    )
    return result.stdout.strip()


def _repo(tmp_path: Path) -> Path:
    root = tmp_path / "repo"
    root.mkdir()
    _git(root, "init")
    _git(root, "config", "user.email", "test@example.invalid")
    _git(root, "config", "user.name", "JAMES Test")
    (root / "tracked.txt").write_text("baseline\n", encoding="utf-8")
    _git(root, "add", "tracked.txt")
    _git(root, "commit", "-m", "baseline")
    return root


def test_checkpoint_requires_clean_tree(tmp_path: Path) -> None:
    root = _repo(tmp_path)
    manager = GitCheckpointManager(root)
    (root / "tracked.txt").write_text("dirty\n", encoding="utf-8")
    with pytest.raises(RuntimeError, match="working tree must be clean"):
        manager.create_checkpoint()


def test_failed_verification_cannot_commit(tmp_path: Path) -> None:
    root = _repo(tmp_path)
    manager = GitCheckpointManager(root)
    checkpoint = manager.create_checkpoint()

    (root / "tracked.txt").write_text("candidate\n", encoding="utf-8")
    result = manager.verify(("python", "-c", "raise SystemExit(7)"))
    assert not result.success

    with pytest.raises(RuntimeError, match="verification failed"):
        manager.commit_verified(
            checkpoint,
            message="candidate",
            verification=result,
        )


def test_successful_verification_commits_and_rollback_restores(tmp_path: Path) -> None:
    root = _repo(tmp_path)
    manager = GitCheckpointManager(root)
    checkpoint = manager.create_checkpoint()

    (root / "tracked.txt").write_text("candidate\n", encoding="utf-8")
    result = manager.verify(("python", "-c", "print('ok')"))
    assert result.success

    commit = manager.commit_verified(
        checkpoint,
        message="candidate",
        verification=result,
    )
    assert commit
    assert (root / "tracked.txt").read_text(encoding="utf-8") == "candidate\n"

    manager.rollback(checkpoint)
    assert (root / "tracked.txt").read_text(encoding="utf-8") == "baseline\n"
    assert manager.status() == ""
