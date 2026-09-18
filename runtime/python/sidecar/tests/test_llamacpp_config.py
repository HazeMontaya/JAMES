from james_runtime.config import LlamaCppConfig


def test_llamacpp_config_is_portable():
    config = LlamaCppConfig(
        executable=r"C:\\tools\\llama.cpp\\llama-server.exe",
        working_directory=r"C:\\tools\\llama.cpp",
        auto_start=False,
    )

    assert config.executable.endswith("llama-server.exe")
    assert config.working_directory.endswith("llama.cpp")
    assert config.auto_start is False


def test_llamacpp_defaults_remain_compatible():
    config = LlamaCppConfig()

    assert config.executable == "llama-server"
    assert config.working_directory is None
    assert config.auto_start is True
