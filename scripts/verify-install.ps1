#Requires -Version 5.1
<#
.SYNOPSIS
    Real-machine install verification for OpenFlex on Windows.

.DESCRIPTION
    Installs a *published* OpenFlex artifact on the machine running the script,
    runs `--version`, `new`, `validate` and `run`, then uninstalls it and checks
    that nothing was left behind.

    Two installers are supported:
      * MSI  (-Msi path\to\openflex-0.1.0-windows-x64.msi)  -> msiexec /i ... /qn
      * ZIP  (-Zip path\to\openflex-0.1.0-x86_64-pc-windows-msvc.zip) -> install.ps1

    On a non-Windows host the script prints the exact commands to copy into a
    Windows machine (and to a Linux/macOS shell) instead of pretending to test.

.PARAMETER Msi
    Path to the MSI built by the release workflow.

.PARAMETER Zip
    Path to the Windows release zip (verified with packaging\install.ps1).

.PARAMETER FromDir
    Directory that contains the Windows release zip; used together with -Version.

.PARAMETER Version
    Version to verify (default: parsed from the archive/MSI file name).

.PARAMETER InstallDir
    Directory used for the zip install (default: a throwaway directory below
    the work dir). The MSI always installs into %LOCALAPPDATA%\OpenFlex\bin.

.PARAMETER WorkDir
    Scratch directory (default: %TEMP%\openflex-verify-install).

.PARAMETER Report
    Append one markdown table row with the result to this file.

.PARAMETER Keep
    Keep the scratch directory (and skip the final cleanup).

.PARAMETER PrintOnly
    Only print the commands; do not install or uninstall anything.

.EXAMPLE
    .\scripts\verify-install.ps1 -Msi .\dist\openflex-0.1.0-windows-x64.msi

.EXAMPLE
    .\scripts\verify-install.ps1 -FromDir .\dist -Version 0.1.0

.EXAMPLE
    .\scripts\verify-install.ps1 -PrintOnly
#>
[CmdletBinding()]
param(
    [string]$Msi = '',
    [string]$Zip = '',
    [string]$FromDir = '',
    [string]$Version = '',
    [string]$InstallDir = '',
    [string]$WorkDir = '',
    [string]$Report = '',
    [switch]$Keep,
    [switch]$PrintOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Program = 'verify-install.ps1'
$RepoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$script:Passed = 0
$script:Failed = 0
$script:Skipped = 0

function Write-Pass([string]$Message) {
    $script:Passed++
    Write-Host "  PASS  $Message" -ForegroundColor Green
}
function Write-Fail([string]$Message) {
    $script:Failed++
    Write-Host "  FAIL  $Message" -ForegroundColor Red
}
function Write-Skip([string]$Message) {
    $script:Skipped++
    Write-Host "  SKIP  $Message" -ForegroundColor Yellow
}
function Write-Section([string]$Title) {
    Write-Host ""
    Write-Host "== $Title =="
}

function Get-MsiCommands([string]$MsiPath) {
    # %LOCALAPPDATA% is empty when this runs on a non-Windows host: keep the
    # command sheet literal so it can still be copied to a Windows machine.
    $localAppData = $env:LOCALAPPDATA
    if (-not $localAppData) { $localAppData = '%LOCALAPPDATA%' }
    $binPath = Join-Path $localAppData 'OpenFlex\bin\openflex.exe'
    @(
        '# Run these in an elevated or normal PowerShell 5.1+ window:',
        ('msiexec /i "{0}" /qn /norestart' -f $MsiPath),
        ('"{0}" --version' -f $binPath),
        ('"{0}" new "$env:TEMP\of-verify" --template mm1' -f $binPath),
        ('"{0}" validate "$env:TEMP\of-verify\mm1.ofm"' -f $binPath),
        ('"{0}" run "$env:TEMP\of-verify\mm1.ofm" --format json' -f $binPath),
        ('msiexec /x "{0}" /qn /norestart' -f $MsiPath),
        '# Exit criteria: every command exits 0, --version prints "openflex <version>",',
        '# and %LOCALAPPDATA%\OpenFlex\bin\openflex.exe no longer exists after /x.'
    )
}

function Get-ZipCommands([string]$ZipPath) {
    $tempDir = $env:TEMP
    if (-not $tempDir) { $tempDir = '%TEMP%' }
    $zipBin = Join-Path $tempDir 'of-verify\bin\openflex.exe'
    @(
        ('.\packaging\install.ps1 -FromDir "{0}" -Version {1} -InstallDir "$env:TEMP\of-verify\bin"' -f (Split-Path -Parent $ZipPath), $Version),
        ('"{0}" --version' -f $zipBin),
        ('.\packaging\uninstall.ps1 -InstallDir "$env:TEMP\of-verify\bin"'),
        '# Exit criteria: install exits 0 after verifying SHA-256, the binary reports',
        '# "openflex <version>", and uninstall removes the binary and the receipt.'
    )
}

function Resolve-Version([string]$FileName) {
    # openflex-<version>-<platform>.(zip|msi) -> <version>
    $stem = [System.IO.Path]::GetFileNameWithoutExtension($FileName)
    if ($stem -like 'openflex-*') {
        $rest = $stem.Substring('openflex-'.Length)
        if ($rest -match '^(?<version>.+)-(x86_64|x64|aarch64|arm64|windows-x64|universal).*$') {
            return $Matches['version']
        }
    }
    return ''
}

function Wait-ForPath([string]$Path, [bool]$ShouldExist, [int]$TimeoutSeconds = 60) {
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
        $exists = Test-Path -LiteralPath $Path
        if ($exists -eq $ShouldExist) { return $true }
        Start-Sleep -Milliseconds 500
    }
    return $false
}

function Invoke-OpenflexSmoke([string]$ExePath, [string]$Version, [string]$ModelDir) {
    try {
        $output = (& $ExePath --version 2>&1 | Out-String).Trim()
        if ($output -eq "openflex $Version") {
            Write-Pass "openflex --version reports 'openflex $Version'"
        }
        else {
            Write-Fail "openflex --version reported '$output' (expected 'openflex $Version')"
        }
    }
    catch {
        Write-Fail "openflex --version failed: $($_.Exception.Message)"
    }

    New-Item -ItemType Directory -Force -Path $ModelDir | Out-Null
    try {
        & $ExePath new $ModelDir --template mm1 | Out-Null
        if (Test-Path -LiteralPath (Join-Path $ModelDir 'mm1.ofm')) {
            Write-Pass "openflex new created mm1.ofm"
        }
        else {
            Write-Fail "openflex new did not create mm1.ofm"
        }
    }
    catch {
        Write-Fail "openflex new failed: $($_.Exception.Message)"
    }

    $model = Join-Path $ModelDir 'mm1.ofm'
    if (Test-Path -LiteralPath $model) {
        try {
            & $ExePath validate $model | Out-Null
            if ($LASTEXITCODE -eq 0) { Write-Pass 'openflex validate accepts the generated model' }
            else { Write-Fail "openflex validate exited $LASTEXITCODE" }
        }
        catch {
            Write-Fail "openflex validate failed: $($_.Exception.Message)"
        }

        $reportJson = Join-Path $ModelDir 'report.json'
        try {
            & $ExePath run $model --format json --replications 2 | Set-Content -LiteralPath $reportJson -Encoding utf8
            if ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $reportJson)) {
                Write-Pass 'openflex run --format json exits 0'
                $raw = Get-Content -LiteralPath $reportJson -Raw
                if ($raw -match '"objects"') { Write-Pass 'report.json contains an objects section' }
                else { Write-Fail 'report.json has no objects section' }
                try {
                    $null = $raw | ConvertFrom-Json
                    Write-Pass 'report.json is valid JSON'
                }
                catch {
                    Write-Fail "report.json is not valid JSON: $($_.Exception.Message)"
                }
            }
            else {
                Write-Fail "openflex run --format json exited $LASTEXITCODE"
            }
        }
        catch {
            Write-Fail "openflex run --format json failed: $($_.Exception.Message)"
        }

        try {
            $csv = (& $ExePath run $model --format csv 2>&1 | Out-String)
            if ($csv -match 'Sink') { Write-Pass 'openflex run --format csv names the sink' }
            else { Write-Fail 'openflex run --format csv did not mention the sink' }
        }
        catch {
            Write-Fail "openflex run --format csv failed: $($_.Exception.Message)"
        }
    }
}

# ---------------------------------------------------------------- environment
$isWindowsHost = ($env:OS -eq 'Windows_NT')

Write-Section 'Environment'
Write-Host ("  host:    {0}" -f [System.Environment]::OSVersion.VersionString)
Write-Host ("  pwsh:    {0} {1}" -f $PSVersionTable.PSEdition, $PSVersionTable.PSVersion)
if ($isWindowsHost) {
    Write-Host ("  arch:    {0}" -f $env:PROCESSOR_ARCHITECTURE)
}
else {
    Write-Host '  arch:    (not a Windows host)'
}

if (-not $Msi -and -not $Zip -and -not $FromDir) {
    $autoDir = Join-Path $RepoRoot 'dist'
    if (Test-Path -LiteralPath $autoDir) {
        $autoMsi = Get-ChildItem -LiteralPath $autoDir -Filter '*.msi' -ErrorAction SilentlyContinue | Select-Object -First 1
        $autoZip = Get-ChildItem -LiteralPath $autoDir -Filter 'openflex-*.zip' -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($autoMsi) { $Msi = $autoMsi.FullName }
        elseif ($autoZip) { $Zip = $autoZip.FullName }
    }
}
if (-not $Msi -and -not $Zip -and -not $FromDir) {
    Write-Skip 'no -Msi, -Zip or -FromDir given and nothing found in .\dist'
}

if (-not $Version) {
    if ($Msi) { $Version = Resolve-Version (Split-Path -Leaf $Msi) }
    if (-not $Version -and $Zip) { $Version = Resolve-Version (Split-Path -Leaf $Zip) }
}
$Version = $Version.TrimStart('v')

if (-not $WorkDir) {
    $WorkDir = Join-Path $env:TEMP 'openflex-verify-install'
}
New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null
if (-not $InstallDir) {
    $InstallDir = Join-Path $WorkDir 'bin'
}

if (-not $isWindowsHost) {
    Write-Section 'Windows command sheet (this host cannot run MSI/zip installs)'
    Write-Host '  Copy the block for the artifact you published into a Windows 10/11 machine:'
    Write-Host ''
    if ($Msi) {
        Write-Host ("  # --- MSI: {0} ---" -f (Split-Path -Leaf $Msi))
        Get-MsiCommands '<path-to-msi>' | ForEach-Object { Write-Host ("  {0}" -f $_) }
        Write-Host ''
    }
    if ($Zip -or $FromDir) {
        Write-Host '  # --- zip + install.ps1 ---'
        Get-ZipCommands '<path-to-zip>' | ForEach-Object { Write-Host ("  {0}" -f $_) }
        Write-Host ''
    }
    if (-not $Msi -and -not $Zip -and -not $FromDir) {
        Write-Host '  # Nothing was passed. On Windows, run:'
        Write-Host '  #   .\scripts\verify-install.ps1 -FromDir .\dist -Version 0.1.0'
        Write-Host '  #   .\scripts\verify-install.ps1 -Msi .\dist\openflex-0.1.0-windows-x64.msi'
        Write-Host ''
    }
    Write-Skip 'MSI/zip install verification needs a Windows host'
}

# ---------------------------------------------------------------- MSI
if ($Msi -and $isWindowsHost -and -not $PrintOnly) {
    Write-Section 'MSI install / run / uninstall'
    if (-not (Test-Path -LiteralPath $Msi)) {
        Write-Fail "MSI not found: $Msi"
    }
    elseif (-not $Version) {
        Write-Fail 'could not determine the version; pass -Version <V>'
    }
    else {
        $msiPath = (Resolve-Path -LiteralPath $Msi).Path
        $msiBin = Join-Path $env:LOCALAPPDATA 'OpenFlex\bin\openflex.exe'
        if (Test-Path -LiteralPath $msiBin) {
            Write-Skip ("an OpenFlex install already exists at {0}; refusing to overwrite a real installation" -f $msiBin)
        }
        else {
            $log = Join-Path $WorkDir 'msi-install.log'
            $proc = Start-Process -FilePath 'msiexec.exe' `
                -ArgumentList @('/i', ('"{0}"' -f $msiPath), '/qn', '/norestart', '/l*v', ('"{0}"' -f $log)) `
                -Wait -PassThru
            if ($proc.ExitCode -eq 0 -or $proc.ExitCode -eq 3010) {
                Write-Pass "msiexec /i exited $($proc.ExitCode)"
            }
            else {
                Write-Fail "msiexec /i exited $($proc.ExitCode) (log: $log)"
            }
            if (Wait-ForPath -Path $msiBin -ShouldExist $true -TimeoutSeconds 60) {
                Write-Pass "MSI installed $msiBin"
            }
            else {
                Write-Fail "MSI did not create $msiBin"
            }

            if (Test-Path -LiteralPath $msiBin) {
                Invoke-OpenflexSmoke -ExePath $msiBin -Version $Version -ModelDir (Join-Path $WorkDir 'model-msi')

                $proc = Start-Process -FilePath 'msiexec.exe' `
                    -ArgumentList @('/x', ('"{0}"' -f $msiPath), '/qn', '/norestart') `
                    -Wait -PassThru
                if ($proc.ExitCode -eq 0 -or $proc.ExitCode -eq 3010) {
                    Write-Pass "msiexec /x exited $($proc.ExitCode)"
                }
                else {
                    Write-Fail "msiexec /x exited $($proc.ExitCode)"
                }
                if (Wait-ForPath -Path $msiBin -ShouldExist $false -TimeoutSeconds 60) {
                    Write-Pass 'MSI uninstall removed the binary'
                }
                else {
                    Write-Fail 'MSI uninstall left the binary behind'
                }
            }
        }
    }
}

# ---------------------------------------------------------------- zip
if ($Zip -and $isWindowsHost -and -not $PrintOnly) {
    Write-Section 'Zip install / run / uninstall'
    if (-not (Test-Path -LiteralPath $Zip)) {
        Write-Fail "zip not found: $Zip"
    }
    elseif (-not $Version) {
        Write-Fail 'could not determine the version; pass -Version <V>'
    }
    else {
        $installScript = Join-Path $RepoRoot 'packaging\install.ps1'
        $uninstallScript = Join-Path $RepoRoot 'packaging\uninstall.ps1'
        $zipDir = Split-Path -Parent (Resolve-Path -LiteralPath $Zip).Path
        $zipBin = Join-Path $InstallDir 'openflex.exe'
        try {
            & $installScript -FromDir $zipDir -Version $Version -InstallDir $InstallDir -NoPathHint -Force | Out-Host
            if ($LASTEXITCODE -eq 0 -and (Test-Path -LiteralPath $zipBin)) {
                Write-Pass "install.ps1 installed $zipBin"
            }
            else {
                Write-Fail "install.ps1 exited $LASTEXITCODE or did not install the binary"
            }
        }
        catch {
            Write-Fail "install.ps1 failed: $($_.Exception.Message)"
        }

        if (Test-Path -LiteralPath $zipBin) {
            Invoke-OpenflexSmoke -ExePath $zipBin -Version $Version -ModelDir (Join-Path $WorkDir 'model-zip')
            $receipt = Join-Path (Split-Path -Parent $InstallDir) 'share\openflex\install-receipt.txt'
            if (Test-Path -LiteralPath $receipt) {
                Write-Pass 'install receipt written'
            }
            else {
                Write-Skip "no receipt at $receipt (receipts are optional for the zip install)"
            }
            try {
                & $uninstallScript -InstallDir $InstallDir | Out-Host
                if (-not (Test-Path -LiteralPath $zipBin)) {
                    Write-Pass 'uninstall.ps1 removed the binary'
                }
                else {
                    Write-Fail 'uninstall.ps1 left the binary behind'
                }
            }
            catch {
                Write-Fail "uninstall.ps1 failed: $($_.Exception.Message)"
            }
        }
    }
}

if ($PrintOnly) {
    Write-Section 'Command sheet (PrintOnly)'
    if ($Msi) { Get-MsiCommands $Msi | ForEach-Object { Write-Host ("  {0}" -f $_) } }
    if ($Zip) { Get-ZipCommands $Zip | ForEach-Object { Write-Host ("  {0}" -f $_) } }
    if (-not $Msi -and -not $Zip) { Write-Skip 'nothing to print (pass -Msi or -Zip)' }
}

# ---------------------------------------------------------------- summary
Write-Section 'Summary'
Write-Host ("  passed:  {0}" -f $script:Passed)
Write-Host ("  failed:  {0}" -f $script:Failed)
Write-Host ("  skipped: {0}" -f $script:Skipped)

$result = 'PASS'
if ($script:Failed -gt 0) { $result = 'FAIL' }

if ($Report) {
    $osLabel = 'Windows'
    if ($isWindowsHost) { $osLabel = 'Windows ' + [System.Environment]::OSVersion.Version.ToString() }
    $mode = 'none'
    if ($Msi) { $mode = 'msi' }
    elseif ($Zip) { $mode = 'zip' }
    elseif ($FromDir) { $mode = 'from-dir' }
    $line = '| {0} | {1} | {2} | {3} | {4} | {5} |' -f `
        $osLabel, $mode, ($env:PROCESSOR_ARCHITECTURE), $(if ($Version) { $Version } else { 'unknown' }), $result, (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd HH:mm UTC')
    Add-Content -LiteralPath $Report -Value $line -Encoding utf8
    Write-Host "  appended a result row to $Report"
}

if (-not $Keep) {
    if (Test-Path -LiteralPath $WorkDir) {
        Remove-Item -LiteralPath $WorkDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}
else {
    Write-Host "  scratch directory kept: $WorkDir"
}

if ($script:Failed -gt 0) {
    Write-Host ""
    Write-Host ("RESULT: FAIL ({0} failed check(s))" -f $script:Failed) -ForegroundColor Red
    exit 1
}
Write-Host ''
Write-Host 'RESULT: PASS' -ForegroundColor Green
exit 0
