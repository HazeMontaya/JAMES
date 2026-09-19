param(
  [switch]$Full
)

$ErrorActionPreference = "Stop"
$Repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

Write-Host "=== JAMES PREFLIGHT ===" -ForegroundColor Cyan
Write-Host "Repo: $Repo"

$rustup = Get-Command rustup -ErrorAction SilentlyContinue
if (-not $rustup) { throw "rustup not found" }

$env:PATH = "$HOME\.cargo\bin;$env:PATH"
$env:RUSTC_WRAPPER = ""
$env:CARGO_INCREMENTAL = "0"

$active = rustup show active-toolchain
$targets = rustup target list --installed
if ($active -notmatch "stable-x86_64-pc-windows-msvc") {
  throw "JAMES requires stable-x86_64-pc-windows-msvc. Active: $active"
}
if ($targets -notmatch "x86_64-pc-windows-msvc") {
  rustup target add x86_64-pc-windows-msvc
}
$lib = rustc --print target-libdir --target x86_64-pc-windows-msvc
if (-not (Test-Path $lib)) { throw "Rust MSVC target library missing: $lib" }

Write-Host "Rust toolchain: OK" -ForegroundColor Green

Push-Location (Join-Path $Repo "runtime/core")
try {
  cargo check --workspace --target x86_64-pc-windows-msvc
  if ($Full) { cargo test --workspace --target x86_64-pc-windows-msvc }
} finally { Pop-Location }

Push-Location (Join-Path $Repo "runtime/modules")
try {
  cargo check --workspace --target x86_64-pc-windows-msvc
  if ($Full) { cargo test --workspace --target x86_64-pc-windows-msvc }
} finally { Pop-Location }

Push-Location (Join-Path $Repo "runtime/python/sidecar")
try {
  python -m compileall -q james_runtime
  if ($Full) { python -m pytest -q }
} finally { Pop-Location }

Push-Location (Join-Path $Repo "runtime/tools/discovery")
try {
  npm run build
  if ($Full) { npm test }
} finally { Pop-Location }

Write-Host "=== JAMES PREFLIGHT PASSED ===" -ForegroundColor Green
