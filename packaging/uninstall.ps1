#Requires -Version 5.1
<#
.SYNOPSIS
    Uninstall OpenFlex from the current user's profile on Windows.

.DESCRIPTION
    Removes openflex.exe installed by packaging/install.ps1, removes the
    install directory from the *user* PATH, and deletes the install directory
    when it is empty. The machine-wide PATH is never touched and nothing is
    removed outside the install directory.

.PARAMETER InstallDir
    Directory that contains openflex.exe.
    Default: $env:LOCALAPPDATA\OpenFlex\bin

.PARAMETER DryRun
    Print what would be removed and change nothing.

.EXAMPLE
    .\packaging\uninstall.ps1

.EXAMPLE
    .\packaging\uninstall.ps1 -InstallDir C:\temp\openflex\bin -DryRun
#>
[CmdletBinding()]
param(
    [string]$InstallDir = '',
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Program = 'uninstall.ps1'
if (-not $InstallDir) {
    $InstallDir = Join-Path $env:LOCALAPPDATA 'OpenFlex\bin'
}
$InstallDir = [System.IO.Path]::GetFullPath($InstallDir).TrimEnd('\')

function Write-Info {
    param([string]$Message)
    Write-Host "${Program}: $Message" -ForegroundColor Cyan
}

function Remove-UserPathEntry {
    param([string]$Directory)
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ([string]::IsNullOrEmpty($userPath)) { return $false }
    $segments = @($userPath -split ';' | Where-Object { $_ -and $_.Trim() })
    $kept = @($segments | Where-Object { $_.TrimEnd('\') -ine $Directory.TrimEnd('\') })
    if ($kept.Count -eq $segments.Count) { return $false }
    if ($DryRun) { return $true }
    $newPath = $kept -join ';'
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    $env:Path = (@($env:Path -split ';' | Where-Object { $_ -and $_.TrimEnd('\') -ine $Directory.TrimEnd('\') })) -join ';'
    return $true
}

$removed = $false
$target = Join-Path $InstallDir 'openflex.exe'
$receiptRoot = Split-Path -Parent $InstallDir
$receipt = Join-Path $receiptRoot 'install-receipt.txt'

# The receipt records the exact binary path (the user may have installed with
# a different -InstallDir for the binary and the data directory).
$binary = $target
if (Test-Path -LiteralPath $receipt) {
    foreach ($line in Get-Content -LiteralPath $receipt) {
        if ($line -match '^binary=(.+)$') {
            $recorded = $Matches[1].Trim()
            if ((Split-Path -Leaf $recorded) -eq 'openflex.exe') { $binary = $recorded }
        }
    }
}

if (Test-Path -LiteralPath $binary) {
    if ($DryRun) {
        Write-Info "[dry-run] would remove $binary"
    }
    else {
        Remove-Item -LiteralPath $binary -Force
        Write-Info "removed $binary"
    }
    $removed = $true
}
else {
    Write-Info "no binary at $binary"
}

if (Test-Path -LiteralPath $receipt) {
    if ($DryRun) {
        Write-Info "[dry-run] would remove $receipt"
    }
    else {
        Remove-Item -LiteralPath $receipt -Force -ErrorAction SilentlyContinue
        Write-Info "removed $receipt"
    }
    $removed = $true
}

$pathChanged = Remove-UserPathEntry -Directory $InstallDir
if ($pathChanged) {
    if ($DryRun) {
        Write-Info "[dry-run] would remove $InstallDir from the user PATH"
    }
    else {
        Write-Info "removed $InstallDir from the user PATH (restart the terminal)"
    }
}
else {
    Write-Info "$InstallDir is not on the user PATH"
}

foreach ($dir in @($InstallDir, $receiptRoot)) {
    if (-not (Test-Path -LiteralPath $dir -PathType Container)) { continue }
    $children = @(Get-ChildItem -LiteralPath $dir -Force)
    if ($children.Count -gt 0) {
        Write-Info "$dir is not empty; leaving it in place"
        continue
    }
    if ($DryRun) {
        Write-Info "[dry-run] would remove empty directory $dir"
    }
    else {
        Remove-Item -LiteralPath $dir -Force
        Write-Info "removed empty directory $dir"
    }
}

if (-not $removed) {
    Write-Info "nothing to uninstall under $InstallDir"
}
if ($DryRun) {
    Write-Info '[dry-run] nothing was changed'
}
Write-Info 'done'
