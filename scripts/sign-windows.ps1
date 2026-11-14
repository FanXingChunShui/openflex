#Requires -Version 5.1
<#
.SYNOPSIS
    Sign OpenFlex Windows binaries and installers with signtool (Authenticode).

.DESCRIPTION
    Wraps `signtool sign /fd SHA256 /tr <timestamp> /td SHA256` for one or more
    files, then verifies the signature with `signtool verify /pa /v`.

    The certificate can come from a .pfx file (-CertificatePath plus
    -CertificatePassword) or from the current user's certificate store
    (-Thumbprint). Without a certificate the script refuses to run and prints
    what is missing; use -DryRun to see the exact command line, or -PrintOnly
    on a non-Windows host to get a command sheet.

.PARAMETER Path
    One or more files to sign (openflex.exe, the MSI, ...).

.PARAMETER CertificatePath
    Path to a .pfx/.p12 file containing the code-signing certificate.

.PARAMETER CertificatePassword
    Password for -CertificatePath (or $env:WINDOWS_CERTIFICATE_PASSWORD).

.PARAMETER Thumbprint
    Thumbprint of a certificate already present in Cert:\CurrentUser\My.

.PARAMETER TimestampUrl
    RFC 3161 timestamp server (default: http://timestamp.digicert.com).

.PARAMETER SigntoolPath
    Explicit path to signtool.exe (default: newest one in the Windows 10/11 SDK).

.PARAMETER DryRun
    Print the signtool commands without running them.

.PARAMETER Verify
    Only verify existing signatures; do not sign.

.PARAMETER PrintOnly
    Print a command sheet and exit (used on non-Windows hosts).

.EXAMPLE
    .\scripts\sign-windows.ps1 -Path .\dist\openflex.exe, .\dist\openflex-0.1.0-windows-x64.msi `
        -CertificatePath .\cert.pfx -CertificatePassword $env:WINDOWS_CERTIFICATE_PASSWORD

.EXAMPLE
    .\scripts\sign-windows.ps1 -Path .\dist\openflex.exe -Thumbprint ABCDEF... -Verify
#>
[CmdletBinding()]
param(
    [string[]]$Path = @(),
    [string]$CertificatePath = '',
    [string]$CertificatePassword = $env:WINDOWS_CERTIFICATE_PASSWORD,
    [string]$Thumbprint = '',
    [string]$TimestampUrl = 'http://timestamp.digicert.com',
    [string]$SigntoolPath = '',
    [switch]$DryRun,
    [switch]$Verify,
    [switch]$PrintOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Program = 'sign-windows.ps1'
$script:Passed = 0
$script:Failed = 0

function Write-Pass([string]$Message) {
    $script:Passed++
    Write-Host "  PASS  $Message" -ForegroundColor Green
}
function Write-Fail([string]$Message) {
    $script:Failed++
    Write-Host "  FAIL  $Message" -ForegroundColor Red
}
function Write-Info([string]$Message) {
    Write-Host "$Program`: $Message"
}

function Find-Signtool {
    if ($SigntoolPath) {
        if (Test-Path -LiteralPath $SigntoolPath) { return (Resolve-Path -LiteralPath $SigntoolPath).Path }
        throw "signtool.exe not found at $SigntoolPath"
    }
    $command = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }

    $roots = @()
    if ($env:ProgramFiles) { $roots += (Join-Path $env:ProgramFiles 'Windows Kits\10\bin') }
    if (${env:ProgramFiles(x86)}) { $roots += (Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin') }
    foreach ($root in $roots) {
        if (-not (Test-Path -LiteralPath $root)) { continue }
        $found = Get-ChildItem -LiteralPath $root -Recurse -Filter 'signtool.exe' -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match '\\x64\\' } |
            Sort-Object -Property FullName -Descending |
            Select-Object -First 1
        if ($found) { return $found.FullName }
    }
    throw 'signtool.exe was not found; install the Windows SDK or pass -SigntoolPath'
}

$isWindowsHost = ($env:OS -eq 'Windows_NT')

Write-Host ''
Write-Host '== Environment =='
Write-Host ("  host:    {0}" -f [System.Environment]::OSVersion.VersionString)
Write-Host ("  files:   {0}" -f ($(if ($Path.Count -gt 0) { $Path -join ', ' } else { '(none)' })))

if ($PrintOnly -or -not $isWindowsHost) {
    Write-Host ''
    Write-Host '== Windows command sheet (signtool is Windows-only) =='
    Write-Host '  # 1) Sign the binary before zipping it, and the MSI after wix build:'
    Write-Host '  signtool.exe sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /f cert.pfx /p %WINDOWS_CERTIFICATE_PASSWORD% openflex.exe'
    Write-Host '  signtool.exe sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 /f cert.pfx /p %WINDOWS_CERTIFICATE_PASSWORD% openflex-0.1.0-windows-x64.msi'
    Write-Host '  # 2) Verify (exit code 0 means the signature is valid):'
    Write-Host '  signtool.exe verify /pa /v openflex.exe'
    Write-Host '  signtool.exe verify /pa /v openflex-0.1.0-windows-x64.msi'
    Write-Host '  # 3) Optional: an EV certificate on a hardware token needs a CSP/PKCS#11'
    Write-Host '  #    provider or Azure Key Vault; use `/csp` + `/k` or signtool''s /dlib.'
    Write-Host ''
    Write-Host 'RESULT: PASS (command sheet only)'
    exit 0
}

if ($Path.Count -eq 0) {
    Write-Fail 'no -Path given; nothing to sign'
}
else {
    try {
        $signtool = Find-Signtool
        Write-Info "using $signtool"
    }
    catch {
        Write-Fail $_.Exception.Message
        $signtool = ''
    }

    foreach ($file in $Path) {
        if (-not (Test-Path -LiteralPath $file)) {
            Write-Fail "file not found: $file"
            continue
        }
        $full = (Resolve-Path -LiteralPath $file).Path

        if ($Verify) {
            if (-not $signtool) { continue }
            try {
                & $signtool verify /pa /v $full | Out-Host
                if ($LASTEXITCODE -eq 0) { Write-Pass "signature verifies: $(Split-Path -Leaf $full)" }
                else { Write-Fail "signature verification failed: $(Split-Path -Leaf $full) (exit $LASTEXITCODE)" }
            }
            catch {
                Write-Fail "signtool verify failed: $($_.Exception.Message)"
            }
            continue
        }

        if (-not $CertificatePath -and -not $Thumbprint) {
            Write-Fail 'no certificate: pass -CertificatePath/-CertificatePassword or -Thumbprint, or -DryRun to print the command'
            continue
        }
        if ($CertificatePath -and -not (Test-Path -LiteralPath $CertificatePath)) {
            Write-Fail "certificate not found: $CertificatePath"
            continue
        }
        if ($CertificatePath -and -not $CertificatePassword) {
            Write-Fail 'CertificatePassword is empty; set $env:WINDOWS_CERTIFICATE_PASSWORD or pass -CertificatePassword'
            continue
        }

        $signArgs = @('sign', '/fd', 'SHA256', '/tr', $TimestampUrl, '/td', 'SHA256')
        if ($CertificatePath) { $signArgs += @('/f', (Resolve-Path -LiteralPath $CertificatePath).Path) }
        if ($CertificatePath -and $CertificatePassword) { $signArgs += @('/p', $CertificatePassword) }
        if ($Thumbprint) { $signArgs += @('/sha1', $Thumbprint) }
        $signArgs += $full

        if ($DryRun) {
            $display = 'signtool.exe ' + ($signArgs -join ' ')
            if ($CertificatePassword) { $display = $display.Replace($CertificatePassword, '***') }
            Write-Host ("  [dry-run] " + $display)
            Write-Pass "dry-run: would sign $(Split-Path -Leaf $full)"
            continue
        }
        if (-not $signtool) { continue }

        $signExit = 1
        try {
            & $signtool @signArgs | Out-Host
            $signExit = $LASTEXITCODE
        }
        catch {
            Write-Fail "signtool sign failed: $($_.Exception.Message)"
        }
        if ($signExit -eq 0) { Write-Pass "signed $(Split-Path -Leaf $full)" }
        else { Write-Fail "signtool sign failed for $(Split-Path -Leaf $full) (exit $signExit)" }

        if ($signExit -eq 0) {
            try {
                & $signtool verify /pa /v $full | Out-Host
                if ($LASTEXITCODE -eq 0) { Write-Pass "signature verifies: $(Split-Path -Leaf $full)" }
                else { Write-Fail "signature verification failed: $(Split-Path -Leaf $full)" }
            }
            catch {
                Write-Fail "signtool verify failed: $($_.Exception.Message)"
            }
        }
    }
}

Write-Host ''
Write-Host '== Summary =='
Write-Host ("  passed: {0}" -f $script:Passed)
Write-Host ("  failed: {0}" -f $script:Failed)
if ($script:Failed -gt 0) {
    Write-Host ''
    Write-Host ("RESULT: FAIL ({0} failed check(s))" -f $script:Failed) -ForegroundColor Red
    exit 1
}
Write-Host ''
Write-Host 'RESULT: PASS' -ForegroundColor Green
exit 0
