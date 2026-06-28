#requires -Version 5
# Yato Gradient - dev install
#   Builds the cdylib, renames the .dll to YatoGradient.aex, copies it to the
#   shared After Effects MediaCore plug-ins folder (needs Administrator).
#
# Usage (run from an *elevated* PowerShell):
#   powershell -ExecutionPolicy Bypass -File .\install.ps1            # debug build
#   powershell -ExecutionPolicy Bypass -File .\install.ps1 -Release   # release build
param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$proj = $PSScriptRoot

# 1) build
Push-Location $proj
try {
    if ($Release) { cargo build --release } else { cargo build }
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }
} finally {
    Pop-Location
}

# 2) locate artifact (crate name hyphens become underscores)
$sub = if ($Release) { "release" } else { "debug" }
$dll = Join-Path $proj "target\$sub\yato_ae_gradient.dll"
if (-not (Test-Path $dll)) { throw "Build artifact not found: $dll" }

# 3) copy to MediaCore as .aex
$dest = "C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore"
if (-not (Test-Path $dest)) { throw "Plug-in folder not found: $dest" }
$aex = Join-Path $dest "YatoGradient.aex"

try {
    Copy-Item $dll $aex -Force
} catch {
    Write-Error ("Copy failed - run this script from an elevated (Administrator) PowerShell. " + $_.Exception.Message)
    exit 1
}

Write-Host "Installed: $aex"
Write-Host "Restart After Effects, then: Effects > Yato > Yato Gradient"
