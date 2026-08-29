[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Installer,
    [string]$EvidencePath = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Resolve-InstalledApplication {
    $candidates = @(
        (Join-Path $env:LOCALAPPDATA "DocFlow\docflow.exe"),
        (Join-Path $env:LOCALAPPDATA "Programs\DocFlow\docflow.exe"),
        (Join-Path $env:ProgramFiles "DocFlow\docflow.exe")
    )
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }

    $registryRoots = @(
        "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
        "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*"
    )
    foreach ($entry in Get-ItemProperty $registryRoots -ErrorAction SilentlyContinue) {
        if ($entry.DisplayName -eq "DocFlow" -and $entry.InstallLocation) {
            $candidate = Join-Path $entry.InstallLocation "docflow.exe"
            if (Test-Path -LiteralPath $candidate -PathType Leaf) {
                return (Resolve-Path -LiteralPath $candidate).Path
            }
        }
    }
    throw "The NSIS installer completed without a discoverable DocFlow executable."
}

$installerPath = (Resolve-Path -LiteralPath $Installer).Path
$existing = @(
    (Join-Path $env:LOCALAPPDATA "DocFlow\docflow.exe"),
    (Join-Path $env:LOCALAPPDATA "Programs\DocFlow\docflow.exe"),
    (Join-Path $env:ProgramFiles "DocFlow\docflow.exe")
) | Where-Object { Test-Path -LiteralPath $_ -PathType Leaf }
if ($existing) {
    throw "Installer smoke requires a clean DocFlow install location: $existing"
}

$installTimer = [System.Diagnostics.Stopwatch]::StartNew()
$install = Start-Process -FilePath $installerPath -ArgumentList "/S" -Wait -PassThru
$installTimer.Stop()
if ($install.ExitCode -ne 0) {
    throw "NSIS silent install failed with exit code $($install.ExitCode)."
}

$application = Resolve-InstalledApplication
$installRoot = Split-Path -Parent $application
$applicationHash = (Get-FileHash -LiteralPath $application -Algorithm SHA256).Hash.ToLowerInvariant()
$startupTimer = [System.Diagnostics.Stopwatch]::StartNew()
$gui = Start-Process -FilePath $application -PassThru
try {
    if (-not $gui.WaitForInputIdle(30000)) {
        throw "Installed application GUI did not become input-idle within 30 seconds."
    }
    $startupTimer.Stop()
    $coldStartMilliseconds = $startupTimer.ElapsedMilliseconds
    Start-Sleep -Seconds 5
    $gui.Refresh()
    if ($gui.HasExited) {
        throw "Installed application GUI exited during bootstrap with code $($gui.ExitCode)."
    }
    $peakWorkingSetBytes = [int64]$gui.PeakWorkingSet64
}
finally {
    $gui.Refresh()
    if (-not $gui.HasExited) {
        Stop-Process -Id $gui.Id -Force
        if (-not $gui.WaitForExit(15000)) {
            throw "Installed application GUI did not terminate after the smoke check."
        }
    }
}
$report = Join-Path $env:RUNNER_TEMP "docflow-installed-smoke-$([guid]::NewGuid()).json"
$packageSmokeSha256 = ""
try {
    $smoke = Start-Process -FilePath $application -ArgumentList @(
        "--package-smoke",
        "`"$report`""
    ) -Wait -PassThru
    if ($smoke.ExitCode -ne 0) {
        throw "Installed application smoke failed with exit code $($smoke.ExitCode)."
    }
    if (-not (Test-Path -LiteralPath $report -PathType Leaf)) {
        throw "Installed application smoke exited without producing its report."
    }
    $packageSmokeSha256 = (Get-FileHash -LiteralPath $report -Algorithm SHA256).Hash.ToLowerInvariant()
    Get-Content -LiteralPath $report
    & node tools/package_audit.mjs package $installRoot
    if ($LASTEXITCODE -ne 0) {
        throw "Installed package content audit failed with exit code $LASTEXITCODE."
    }
}
finally {
    Remove-Item -LiteralPath $report -Force -ErrorAction SilentlyContinue
}

$uninstaller = Join-Path $installRoot "uninstall.exe"
if (-not (Test-Path -LiteralPath $uninstaller -PathType Leaf)) {
    $uninstaller = Get-ChildItem -LiteralPath $installRoot -File -Filter "*uninstall*.exe" |
        Select-Object -First 1 -ExpandProperty FullName
}
if (-not $uninstaller) {
    throw "Installed DocFlow package has no discoverable NSIS uninstaller."
}
$uninstall = Start-Process -FilePath $uninstaller -ArgumentList "/S" -Wait -PassThru
if ($uninstall.ExitCode -ne 0) {
    throw "NSIS silent uninstall failed with exit code $($uninstall.ExitCode)."
}
Start-Sleep -Seconds 2
if (Test-Path -LiteralPath $application -PathType Leaf) {
    throw "NSIS uninstall left the DocFlow executable installed: $application"
}

if ($EvidencePath) {
    $evidenceDirectory = Split-Path -Parent $EvidencePath
    if ($evidenceDirectory) {
        New-Item -ItemType Directory -Force -Path $evidenceDirectory | Out-Null
    }
    $evidence = [ordered]@{
        schemaVersion = 1
        result = "pass"
        installerFileName = [System.IO.Path]::GetFileName($installerPath)
        installerSha256 = (Get-FileHash -LiteralPath $installerPath -Algorithm SHA256).Hash.ToLowerInvariant()
        installerSizeBytes = (Get-Item -LiteralPath $installerPath).Length
        applicationSha256 = $applicationHash
        installMilliseconds = $installTimer.ElapsedMilliseconds
        coldStartMilliseconds = $coldStartMilliseconds
        peakWorkingSetBytes = $peakWorkingSetBytes
        packageSmokeSha256 = $packageSmokeSha256
        packageAuditPassed = $true
        uninstallVerified = $true
    }
    $evidence | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $EvidencePath -Encoding utf8
}

Write-Host "Automated Windows installer install/startup/OCR/apply/undo/uninstall smoke: OK"
Write-Warning "Microsoft Office/Excel COM and physical/network printer certification remain manual host gates."
