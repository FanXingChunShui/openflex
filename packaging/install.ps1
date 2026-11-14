#Requires -Version 5.1
<#
.SYNOPSIS
    Install OpenFlex on Windows.

.DESCRIPTION
    Downloads the official OpenFlex release zip (or installs from a local
    release directory), verifies its SHA-256 checksum, copies openflex.exe into
    a user-writable directory and appends that directory to the *user* PATH.

    No administrator rights are required and nothing outside the user profile
    is modified.

.PARAMETER Version
    Release version to install, e.g. 0.1.0 or v0.1.0. Defaults to the latest
    GitHub release.

.PARAMETER InstallDir
    Directory that receives openflex.exe.
    Default: $env:LOCALAPPDATA\OpenFlex\bin

.PARAMETER FromDir
    Install from a local release directory (containing
    openflex-<version>-<platform>.zip and optionally SHA256SUMS.txt) instead of
    downloading from GitHub.

.PARAMETER BaseUrl
    Release download base URL.
    Default: https://github.com/openflex/openflex/releases/download

.PARAMETER DryRun
    Print what would happen and change nothing.

.PARAMETER Force
    Reinstall even when the same version is already installed.

.PARAMETER RequireChecksum
    Fail when no SHA256SUMS.txt is available instead of warning.

.PARAMETER NoPathHint
    Do not touch the user PATH and do not print PATH instructions.

.EXAMPLE
    irm https://raw.githubusercontent.com/openflex/openflex/main/packaging/install.ps1 | iex

.EXAMPLE
    .\packaging\install.ps1 -Version 0.1.0 -InstallDir "$env:LOCALAPPDATA\OpenFlex\bin"

.EXAMPLE
    .\packaging\install.ps1 -FromDir .\dist -InstallDir C:\temp\openflex\bin
#>
[CmdletBinding()]
param(
    [string]$Version = '',
    [string]$InstallDir = '',
    [string]$FromDir = '',
    [string]$BaseUrl = '',
    [switch]$DryRun,
    [switch]$Force,
    [switch]$RequireChecksum,
    [switch]$NoPathHint
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Program = 'install.ps1'
$Repo = if ($env:OPENFLEX_REPO) { $env:OPENFLEX_REPO } else { 'openflex/openflex' }
$ApiUrl = if ($env:OPENFLEX_API_URL) { $env:OPENFLEX_API_URL } else { "https://api.github.com/repos/$Repo/releases/latest" }
if (-not $BaseUrl) {
    $BaseUrl = if ($env:OPENFLEX_BASE_URL) { $env:OPENFLEX_BASE_URL } else { "https://github.com/$Repo/releases/download" }
}
if (-not $InstallDir) {
    $InstallDir = Join-Path $env:LOCALAPPDATA 'OpenFlex\bin'
}
$InstallDir = [System.IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
if ($FromDir) {
    if (-not (Test-Path -LiteralPath $FromDir -PathType Container)) {
        Write-Host "$Program: error: -FromDir: no such directory: $FromDir" -ForegroundColor Red
        exit 1
    }
    $FromDir = (Resolve-Path -LiteralPath $FromDir).Path
}

function Write-Info {
    param([string]$Message)
    Write-Host "$Program: $Message" -ForegroundColor Cyan
}

function Fail {
    param([string]$Message)
    Write-Host "$Program: error: $Message" -ForegroundColor Red
    exit 1
}

function Get-OpenFlexArch {
    $arch = $null
    try {
        $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    }
    catch {
        $arch = $null
    }
    if (-not $arch) { $arch = $env:PROCESSOR_ARCHITECTURE }
    if (-not $arch) { Fail 'could not detect the CPU architecture' }
    switch -Regex ($arch) {
        '^(X64|AMD64|x86_64)$' { return 'x86_64' }
        '^(Arm64|ARM64|aarch64)$' { return 'aarch64' }
        default { Fail "unsupported CPU architecture '$arch'; OpenFlex publishes x86_64 and aarch64 Windows builds" }
    }
}

function Get-OpenFlexPlatform {
    param([string]$Arch)
    if ($Arch -eq 'aarch64') { return 'aarch64-pc-windows-msvc' }
    return 'x86_64-pc-windows-msvc'
}

function Resolve-OpenFlexVersion {
    if ($Version) { return ($Version -replace '^v', '') }
    Write-Info "resolving the latest release from $ApiUrl"
    try {
        $release = Invoke-RestMethod -Uri $ApiUrl -UseBasicParsing -TimeoutSec 30
    }
    catch {
        Fail "could not resolve the latest version (network, proxy or rate limit?): $($_.Exception.Message). Pass -Version <V>."
    }
    if (-not $release -or -not $release.tag_name) {
        Fail 'the release API did not return a tag_name; pass -Version <V>'
    }
    return ($release.tag_name -replace '^v', '')
}

function Save-RemoteFile {
    param([string]$Uri, [string]$Path)
    Invoke-WebRequest -Uri $Uri -OutFile $Path -UseBasicParsing -TimeoutSec 120
}

function Get-ExpectedHash {
    param([string]$SumsPath, [string]$ArchiveName)
    if (-not (Test-Path -LiteralPath $SumsPath)) { return $null }
    foreach ($line in Get-Content -LiteralPath $SumsPath) {
        if ($line -match '^([0-9a-fA-F]{64})\s+\*?(.+)$') {
            if ($Matches[2].Trim() -eq $ArchiveName) { return $Matches[1].ToLowerInvariant() }
        }
    }
    return $null
}

function Set-UserPathEntry {
    param([string]$Directory, [switch]$DryRun)
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($null -eq $userPath) { $userPath = '' }
    $segments = @($userPath -split ';' | Where-Object { $_ -and $_.Trim() })
    $already = @($segments | Where-Object { $_.TrimEnd('\') -ieq $Directory.TrimEnd('\') })
    if ($already.Count -gt 0) {
        Write-Info "$Directory is already on the user PATH"
        return $false
    }
    if ($DryRun) {
        Write-Info "[dry-run] would append $Directory to the user PATH"
        return $true
    }
    $root = Split-Path -Parent $Directory
    if ($root -and (Test-Path -LiteralPath $root -PathType Container)) {
        $backup = Join-Path $root 'path-backup.txt'
        if (-not (Test-Path -LiteralPath $backup)) {
            Set-Content -LiteralPath $backup -Value $userPath -Encoding ASCII
        }
    }
    $newPath = (@($segments) + $Directory) -join ';'
    try {
        [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    }
    catch {
        Write-Warning "$Program: could not update the user PATH automatically: $($_.Exception.Message)"
        Write-Host "  Add this directory to your PATH manually: $Directory"
        return $false
    }
    $env:Path = "$Directory;$env:Path"
    Write-Info "added $Directory to the user PATH (new terminals pick it up automatically)"
    return $true
}

# ---------------------------------------------------------------- main flow
$arch = Get-OpenFlexArch
$platform = Get-OpenFlexPlatform -Arch $arch
$Version = Resolve-OpenFlexVersion
if ($Version -notmatch '^[0-9A-Za-z][0-9A-Za-z.+_-]*$') {
    Fail "invalid version '$Version'"
}
Write-Info "installing OpenFlex $Version for windows/$arch"
Write-Info "target: $InstallDir\openflex.exe"

$archive = $null
$archiveUrl = $null
if ($FromDir) {
    $candidate = Join-Path $FromDir "openflex-$Version-$platform.zip"
    if (Test-Path -LiteralPath $candidate) {
        $archive = (Resolve-Path -LiteralPath $candidate).Path
    }
    else {
        $fallback = @(Get-ChildItem -LiteralPath $FromDir -Filter 'openflex-*.zip' -File | Sort-Object Name)
        if ($fallback.Count -gt 0) {
            $archive = $fallback[$fallback.Count - 1].FullName
            Write-Warning "$Program: no archive matched this platform; using $archive"
        }
    }
    if (-not $archive) {
        Fail "-FromDir: no openflex-$Version-<platform>.zip found in $FromDir"
    }
    Write-Info "using local archive $archive"
}
else {
    $archiveUrl = "$BaseUrl/v$Version/openflex-$Version-$platform.zip"
    Write-Info "downloading $archiveUrl"
}

if ($DryRun) {
    Write-Host ''
    Write-Host "[dry-run] would install OpenFlex $Version ($platform)"
    if ($archiveUrl) {
        Write-Host "[dry-run]   download: $archiveUrl"
        Write-Host "[dry-run]   verify:   $BaseUrl/v$Version/SHA256SUMS.txt"
    }
    else {
        Write-Host "[dry-run]   source:   $archive"
    }
    Write-Host "[dry-run]   binary:   $InstallDir\openflex.exe"
    Write-Host "[dry-run]   receipt:  $(Split-Path -Parent $InstallDir)\install-receipt.txt"
    if (-not $NoPathHint) { Write-Host "[dry-run]   PATH:     append $InstallDir to the user PATH" }
    Write-Host '[dry-run] nothing was changed'
    return
}

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("openflex-install-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
try {
    if (-not $archive) {
        $archive = Join-Path $tempRoot "openflex-$Version-$platform.zip"
        Save-RemoteFile -Uri $archiveUrl -Path $archive
    }
    if (-not (Test-Path -LiteralPath $archive)) { Fail "archive not found: $archive" }
    $archiveName = Split-Path -Leaf $archive

    # --- checksum
    $sumsPath = $null
    if ($FromDir) {
        $localSums = Join-Path $FromDir 'SHA256SUMS.txt'
        if (Test-Path -LiteralPath $localSums) { $sumsPath = $localSums }
    }
    elseif ($archiveUrl) {
        $sumsUrl = "$BaseUrl/v$Version/SHA256SUMS.txt"
        $tmpSums = Join-Path $tempRoot 'SHA256SUMS.txt'
        try {
            Save-RemoteFile -Uri $sumsUrl -Path $tmpSums
            $sumsPath = $tmpSums
        }
        catch {
            Write-Warning "$Program: could not download $sumsUrl"
        }
    }

    if ($sumsPath) {
        $expected = Get-ExpectedHash -SumsPath $sumsPath -ArchiveName $archiveName
        if (-not $expected) {
            if ($RequireChecksum) { Fail "no checksum entry for $archiveName in $sumsPath" }
            Write-Warning "$Program: no checksum entry for $archiveName in $sumsPath; continuing without verification"
        }
        else {
            $actual = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
            if ($actual -ne $expected) {
                Fail "checksum mismatch for $archiveName: expected $expected, got $actual"
            }
            Write-Info "sha256 verified: $actual"
        }
    }
    elseif ($RequireChecksum) {
        Fail "no SHA256SUMS.txt available to verify $archive"
    }
    else {
        Write-Warning "$Program: no SHA256SUMS.txt available; skipping checksum verification"
    }

    # --- extract
    $extractDir = Join-Path $tempRoot 'extract'
    Expand-Archive -LiteralPath $archive -DestinationPath $extractDir -Force
    $found = @(Get-ChildItem -LiteralPath $extractDir -Recurse -Filter 'openflex.exe' -File)
    if ($found.Count -eq 0) { Fail "the archive does not contain openflex.exe" }
    $sourceExe = $found[0].FullName
    Write-Info "found binary $sourceExe"

    # --- already installed?
    $target = Join-Path $InstallDir 'openflex.exe'
    if ((Test-Path -LiteralPath $target) -and -not $Force) {
        $current = (& $target --version 2>$null) -join ' '
        if ($current -match ('openflex\s+' + [regex]::Escape($Version))) {
            Write-Info "openflex $Version is already installed at $target (use -Force to reinstall)"
            if (-not $NoPathHint) { [void](Set-UserPathEntry -Directory $InstallDir) }
            return
        }
    }

    # --- install
    if (-not (Test-Path -LiteralPath $InstallDir -PathType Container)) {
        New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    }
    Copy-Item -LiteralPath $sourceExe -Destination $target -Force
    Write-Info "installed $target"

    # --- receipt
    $receiptRoot = Split-Path -Parent $InstallDir
    $receipt = Join-Path $receiptRoot 'install-receipt.txt'
    $source = if ($archiveUrl) { $archiveUrl } else { $archive }
    try {
        @(
            "version=$Version"
            "platform=$platform"
            "binary=$target"
            "installed_at=$([DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ'))"
            "source=$source"
        ) | Set-Content -LiteralPath $receipt -Encoding ASCII
        Write-Info "wrote receipt $receipt"
    }
    catch {
        Write-Warning "$Program: could not write the receipt: $($_.Exception.Message)"
    }

    # --- verify
    $versionOutput = (& $target --version 2>&1) -join ' '
    if ($LASTEXITCODE -ne 0) {
        Fail "the installed binary failed to run: $versionOutput"
    }
    if ($versionOutput -notmatch ('openflex\s+' + [regex]::Escape($Version))) {
        Fail "the installed binary did not report version $Version (got: $versionOutput)"
    }
    Write-Info "verified: $versionOutput"
}
finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
}

if (-not $NoPathHint) {
    $changed = Set-UserPathEntry -Directory $InstallDir
    if ($changed) {
        Write-Host ''
        Write-Host 'Restart your terminal (or run the command below) before using openflex:'
        Write-Host "  `$env:Path = `"$InstallDir;`$env:Path`""
    }
}

Write-Host ''
Write-Info 'done. Try: openflex --help'
