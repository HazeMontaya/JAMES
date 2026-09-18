param(
  [switch]$Start
)
$ErrorActionPreference = 'Stop'
$root = 'S:\JAMES'
$repo = 'https://github.com/HazeMontaya/JAMES.git'

if (Test-Path -LiteralPath $root) {
  $gitDir = Join-Path $root '.git'
  if (-not (Test-Path -LiteralPath $gitDir)) {
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $backup = "${root}.local-backup-$stamp"
    Rename-Item -LiteralPath $root -NewName (Split-Path $backup -Leaf)
    Write-Host "Existing non-Git directory moved to $backup"
  }
}

if (-not (Test-Path -LiteralPath $root)) {
  git clone $repo $root
}

Set-Location $root
git fetch --all --prune
git checkout main
git reset --hard origin/main
git clean -fd

Write-Host ''
Write-Host 'JAMES repository synchronized:'
git status --short --branch
git log -1 --oneline

if ($Start) {
  & (Join-Path $root 'ops\scripts\start-james.ps1')
}