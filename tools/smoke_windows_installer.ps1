[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Installer
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

$install = Start-Process -FilePath $installerPath -ArgumentList "/S" -Wait -PassThru
if ($install.ExitCode -ne 0) {
    throw "NSIS silent install failed with exit code $($install.ExitCode)."
}

$application = Resolve-InstalledApplication
$installRoot = Split-Path -Parent $application
$gui = Start-Process -FilePath $application -PassThru
try {
    Start-Sleep -Seconds 5
    $gui.Refresh()
    if ($gui.HasExited) {
        throw "Installed application GUI exited during bootstrap with code $($gui.ExitCode)."
    }
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
try {
    & $application --package-smoke $report
    if ($LASTEXITCODE -ne 0) {
        throw "Installed application smoke failed with exit code $LASTEXITCODE."
    }
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

Write-Host "Automated Windows installer install/startup/OCR/apply/undo/uninstall smoke: OK"
Write-Warning "Microsoft Office/Excel COM and physical/network printer certification remain manual host gates."
