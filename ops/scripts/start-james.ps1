# start-james.ps1 — Baue (falls noetig) und starte JAMES System (Void-UI).
# UI: http://127.0.0.1:38241   |   Beenden: Strg+C
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)

# Toolchain-Fix: explizit die MSVC-Toolchain verwenden, damit Cargo-Subprozesse
# nicht den GNU-rustc aus C:\ProgramData\chocolatey\bin auf PATH erwischen.
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-msvc'
$env:RUSTC = "$env:USERPROFILE\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin\rustc.exe"
$env:RUSTC_WRAPPER = ''
$env:CARGO_INCREMENTAL = ''
# Local Void runs only on 127.0.0.1. Allow the browser client to use the protected API
# without exposing the bearer secret to page JavaScript; production deployments
# should leave this false and use an authenticated client.
$env:JAMES_AUTH_DEV_MODE = 'true'
$cargo = "$env:USERPROFILE\.cargo\bin\cargo.exe"

# Ollama: lokale AI-Inferenz sicherstellen. JAMES' AI-Provider spricht den
# Ollama-kompatiblen Endpoint an; ohne ihn antwortet der Chat ehrlich
# "No models available".
$ollamaExe = (Get-Command ollama -ErrorAction SilentlyContinue).Source
if (-not $ollamaExe) { $ollamaExe = Join-Path $env:LOCALAPPDATA 'Programs\Ollama\ollama.exe' }
$ollamaApi = 'http://127.0.0.1:11434/api/tags'
function Test-Ollama {
    try { Invoke-RestMethod $ollamaApi -TimeoutSec 2 | Out-Null; return $true }
    catch { return $false }
}
if (Test-Ollama) {
    Write-Host 'Ollama laeuft bereits.'
} elseif (Test-Path -LiteralPath $ollamaExe) {
    Write-Host 'Ollama nicht erreichbar - starte ollama serve ...'
    Start-Process -FilePath $ollamaExe -ArgumentList 'serve' -WindowStyle Hidden
    $i = 0
    while ($i -lt 20 -and -not (Test-Ollama)) { Start-Sleep 1; $i++ }
    if (Test-Ollama) {
        Write-Host 'Ollama bereit: http://127.0.0.1:11434'
    } else {
        Write-Host 'Warnung: Ollama startet nicht. Chat meldet dann ehrlich: No models available.'
    }
} else {
    Write-Host ('Warnung: ollama.exe nicht gefunden ({0}). Chat bleibt ohne Modell.' -f $ollamaExe)
}
if (Test-Ollama) {
    $modelCount = @((Invoke-RestMethod $ollamaApi -TimeoutSec 3).models).Count
    if ($modelCount -eq 0) {
        Write-Host 'Hinweis: Ollama hat noch kein Modell. Ziehe eins, z.B.: ollama pull llama3.2:1b'
    }
}

$exe = Join-Path $root 'out\modules\debug\james-system.exe'
if (-not (Test-Path -LiteralPath $exe)) {
    Write-Host 'james-system.exe fehlt - baue zuerst ...'
    Push-Location (Join-Path $root 'runtime\modules')
    try {
        & $cargo build -p james-system
        if ($LASTEXITCODE -ne 0) { throw "Build fehlgeschlagen (exit $LASTEXITCODE)" }
    } finally { Pop-Location }
}

Start-Process powershell -ArgumentList '-NoProfile', '-Command', "Start-Sleep 2; Start-Process 'http://127.0.0.1:38241'"

Write-Host ''
Write-Host 'JAMES System - Void-UI: http://127.0.0.1:38241  (Strg+C zum Beenden)'
Write-Host ''

Push-Location (Join-Path $root 'runtime\modules')
try {
    & $exe
    if ($LASTEXITCODE -ne 0) { Write-Host ('james-system beendet mit exit={0}' -f $LASTEXITCODE) }
} finally { Pop-Location }