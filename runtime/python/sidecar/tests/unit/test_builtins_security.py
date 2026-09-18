import pytest

from james_runtime.tools.builtins import (
    FileList,
    FileReader,
    FileWriter,
    HttpGet,
    PythonExec,
    _public_http_url,
    workspace_root,
)


@pytest.mark.asyncio
async def test_filesystem_tools_are_confined_to_workspace(tmp_path, monkeypatch):
    monkeypatch.setenv("JAMES_WORKSPACE_ROOT", str(tmp_path))
    outside = tmp_path.parent / "outside.txt"
    outside.write_text("secret", encoding="utf-8")

    writer = FileWriter()
    result = await writer._run("../outside.txt", "blocked")
    assert result.success is False
    assert outside.read_text(encoding="utf-8") == "secret"

    inside = await writer._run("nested/file.txt", "hello")
    assert inside.success is True

    reader = FileReader()
    assert (await reader._run("nested/file.txt")).output == "hello"
    assert (await reader._run("../outside.txt")).success is False


@pytest.mark.asyncio
async def test_filesystem_symlink_escape_is_rejected(tmp_path, monkeypatch):
    monkeypatch.setenv("JAMES_WORKSPACE_ROOT", str(tmp_path))
    outside = tmp_path.parent / "james-outside"
    outside.mkdir(exist_ok=True)
    (outside / "secret.txt").write_text("secret", encoding="utf-8")
    link = tmp_path / "escape"
    try:
        link.symlink_to(outside, target_is_directory=True)
    except (OSError, NotImplementedError):
        pytest.skip("symlinks unavailable on this platform")

    result = await FileReader()._run("escape/secret.txt")
    assert result.success is False


@pytest.mark.asyncio
async def test_file_list_does_not_follow_external_symlink(tmp_path, monkeypatch):
    monkeypatch.setenv("JAMES_WORKSPACE_ROOT", str(tmp_path))
    outside = tmp_path.parent / "james-list-outside"
    outside.mkdir(exist_ok=True)
    (outside / "secret.txt").write_text("secret", encoding="utf-8")
    link = tmp_path / "escape"
    try:
        link.symlink_to(outside, target_is_directory=True)
    except (OSError, NotImplementedError):
        pytest.skip("symlinks unavailable on this platform")

    result = await FileList()._run(".")
    assert result.success is True
    assert "secret.txt" not in result.output


@pytest.mark.asyncio
async def test_python_exec_does_not_inherit_unrelated_environment(tmp_path, monkeypatch):
    monkeypatch.setenv("JAMES_WORKSPACE_ROOT", str(tmp_path))
    monkeypatch.setenv("JAMES_TEST_SECRET", "must-not-cross-boundary")

    result = await PythonExec()._run(
        "import os; print(os.environ.get('JAMES_TEST_SECRET', '<missing>'))"
    )
    assert result.success is True
    assert "<missing>" in result.output


def test_http_blocks_private_and_local_destinations():
    for url in (
        "http://127.0.0.1:8080/",
        "http://localhost/",
        "http://10.0.0.1/",
        "http://192.168.1.1/",
        "http://[::1]/",
    ):
        with pytest.raises((PermissionError, ValueError)):
            _public_http_url(url)


def test_http_allows_public_ip_literal():
    assert _public_http_url("https://8.8.8.8/") == "https://8.8.8.8/"
