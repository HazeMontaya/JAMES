<# 
.SYNOPSIS
    JAMES Discovery CLI - PowerShell Wrapper

.DESCRIPTION
    Wrapper script to run the JAMES Discovery TypeScript CLI via Node.js.

.PARAMETER Command
    The command to execute: scan, inventory, capabilities, changes, snapshots, compare, diagnose, export, help

.PARAMETER Args
    Additional arguments for the command.

.EXAMPLE
    .\run.ps1 scan fast
    .\run.ps1 inventory
    .\run.ps1 capabilities
    .\run.ps1 changes
    .\run.ps1 snapshots
    .\run.ps1 compare <prev_id> <curr_id>
    .\run.ps1 diagnose
    .\run.ps1 export json inventory-export.json
#>

param(
    [Parameter(Mandatory=$false, Position=0)]
    [string]$Command = 'help',

    [Parameter(Mandatory=$false, Position=1)]
    [string[]]$Args = @()
)

Set-Location $PSScriptRoot

# Check if Node.js is available
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Error "Node.js not found. Please install Node.js first."
    exit 1
}

# Check if the compiled JS exists, if not compile TypeScript
$cliPath = Join-Path $PSScriptRoot "src\cli\index.ts"
$compiledPath = Join-Path $PSScriptRoot "dist\cli\index.js"

if (-not (Test-Path $compiledPath)) {
    Write-Host "Compiling TypeScript..." -ForegroundColor Yellow
    
    # Check for ts-node or typescript
    if (Get-Command npx -ErrorAction SilentlyContinue) {
        npx ts-node $cliPath $Command $Args
    } elseif (Get-Command ts-node -ErrorAction SilentlyContinue) {
        ts-node $cliPath $Command $Args
    } else {
        Write-Host "Installing ts-node..." -ForegroundColor Yellow
        npm install -g ts-node typescript
        npx ts-node $cliPath $Command $Args
    }
} else {
    # Run compiled version
    node $compiledPath $Command $Args
}

exit $LASTEXITCODE