# JAMES Python Runtime

Inference sidecar for model routing, runtime selection, hardware/VRAM admission, telemetry and gRPC/HTTP integration.

The base installation is intentionally lightweight. Heavy inference engines are optional extras so a Windows installation can start with Ollama or another external provider without compiling vLLM or llama.cpp.

Optional extras: `vllm`, `llamacpp`, `airllm`, `cuda`, `rocm`, `metal`, `vulkan`.

Run tests with `python -m pytest -q` from this directory.
