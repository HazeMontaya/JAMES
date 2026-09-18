# JAMES local inference bootstrap
# Windows PowerShell. Idempotent: safe to rerun.
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Sidecar = Join-Path $Root "runtime\python\sidecar"
$Models = Join-Path $Root ".james\models"
$Model = Join-Path $Models "Qwen2.5-3B-Instruct-Q4_K_M.gguf"

New-Item -ItemType Directory -Force -Path $Models | Out-Null

function Require-Command($Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "$Name was not found in PATH."
    }
}

# Prefer the official llama.cpp Windows package when available.
if (-not (Get-Command llama-server -ErrorAction SilentlyContinue)) {
    if (Get-Command winget -ErrorAction SilentlyContinue) {
        winget install --id llama.cpp -e --accept-source-agreements --accept-package-agreements
    }
}

Require-Command "llama-server"

# Download exactly one small GGUF model. The current upstream model card lists
# this Q4_K_M file at about 1.93 GB and documents llama.cpp usage.
if (-not (Test-Path $Model)) {
    if (-not (Get-Command hf -ErrorAction SilentlyContinue)) {
        python -m pip install --upgrade "huggingface_hub[cli]"
    }
    Require-Command "hf"
    hf download bartowski/Qwen2.5-3B-Instruct-GGUF Qwen2.5-3B-Instruct-Q4_K_M.gguf --local-dir $Models
}

if (-not (Test-Path $Model)) {
    throw "Model download did not produce $Model"
}

Write-Host "JAMES local model ready: $Model"
Write-Host "Next: start the JAMES sidecar from $Sidecar"
