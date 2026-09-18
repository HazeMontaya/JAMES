# JAMES Local Runtime

This is the concrete first-run path for the current Windows workstation profile.

## 1. Bootstrap

From the repository root:

    powershell -ExecutionPolicy Bypass -File .\scripts\bootstrap-local.ps1

The bootstrap installs/uses llama.cpp and downloads the Qwen2.5 3B Instruct Q4_K_M GGUF model into `.james/models`.

The upstream model page currently lists the Q4_K_M file at about 1.93 GB and documents llama.cpp as a supported local runtime. See the model card for the exact source and license.

## 2. Start the sidecar

    cd runtime/python/sidecar
    python -m uvicorn james_runtime.server.fastapi_app:app --host 127.0.0.1 --port 38242

## 3. Verify

    curl http://127.0.0.1:38242/health
    curl http://127.0.0.1:38242/v1/models

## 4. OpenAI-compatible request

    curl http://127.0.0.1:38242/v1/chat/completions -H "Content-Type: application/json" -d '{"model":"qwen2.5-3b-instruct-q4","messages":[{"role":"user","content":"Say hello from JAMES."}],"max_tokens":64}'

The runtime is local-first. Cloud providers are not required for this path.
