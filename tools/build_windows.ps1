[CmdletBinding()]
param(
    [switch]$SkipInstall,
    [switch]$SkipTests,
    [string]$TesseractPath = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$ResourceDir = Join-Path $ProjectRoot "src-tauri\resources\tesseract"

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$File,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$Label
    )

    Write-Host "`n==> $Label"
    & $File @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed with exit code $LASTEXITCODE"
    }
}

function Resolve-RequiredCommand {
    param([Parameter(Mandatory = $true)][string]$Name)

    $command = Get-Command $Name -ErrorAction SilentlyContinue
    if (-not $command) {
        throw "'$Name' was not found. See the Windows prerequisites in README.md."
    }
    return $command.Source
}

function Assert-WindowsGuiExecutable {
    param([Parameter(Mandatory = $true)][string]$Path)

    $bytes = [System.IO.File]::ReadAllBytes((Resolve-Path $Path).Path)
    if ($bytes.Length -lt 96 -or $bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) {
        throw "The built application is not a valid Windows PE executable: $Path"
    }

    $peOffset = [System.BitConverter]::ToInt32($bytes, 0x3C)
    $subsystemOffset = $peOffset + 24 + 68
    if ($peOffset -lt 0 -or $subsystemOffset + 1 -ge $bytes.Length) {
        throw "The built application has an invalid PE header: $Path"
    }

    # IMAGE_SUBSYSTEM_WINDOWS_GUI = 2. A CUI value of 3 opens the command
    # window beside the desktop app and closing it terminates the app process.
    $subsystem = [System.BitConverter]::ToUInt16($bytes, $subsystemOffset)
    if ($subsystem -ne 2) {
        throw "The built application uses PE subsystem $subsystem instead of Windows GUI (2)."
    }
}

function Assert-TesseractResourceEvidence {
    param([Parameter(Mandatory = $true)][string]$Root)

    $nativeLibraries = @(Get-ChildItem $Root -Recurse -File -Filter "*.dll")
    if ($nativeLibraries.Count -eq 0) {
        throw "The staged Tesseract resource has no native dependency DLLs."
    }

    $licenseFiles = @(
        Get-ChildItem $Root -Recurse -File |
            Where-Object { $_.Name -match "^(LICENSE|COPYING|NOTICE)(\.|$)" }
    )
    if ($licenseFiles.Count -eq 0) {
        throw "The staged Tesseract resource has no license evidence."
    }
    $rejectedNames = $licenseFiles |
        Where-Object { $_.Name -match "AGPL|Affero|commercial" }
    $spdxFiles = @(Get-ChildItem $Root -Recurse -File -Filter "*.spdx.json")
    $rejectedSpdx = @()
    if ($spdxFiles.Count -gt 0) {
        $rejectedSpdx = $spdxFiles |
            Select-String -Pattern '"license(Concluded|Declared)"\s*:\s*"[^"]*(AGPL|Affero|LicenseRef-commercial)' -CaseSensitive:$false
    }
    if ($rejectedNames -or $rejectedSpdx) {
        throw "The staged Tesseract resource contains rejected AGPL/commercial license evidence."
    }
}

function Write-ResourceChecksums {
    param([Parameter(Mandatory = $true)][string]$Root)

    $checksumPath = Join-Path $Root "SHA256SUMS"
    $lines = @(
        Get-ChildItem $Root -Recurse -File |
            Where-Object { $_.FullName -ne $checksumPath } |
            Sort-Object FullName |
            ForEach-Object {
                $relative = [System.IO.Path]::GetRelativePath($Root, $_.FullName).Replace("\", "/")
                $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
                "$hash  ./$relative"
            }
    )
    if ($lines.Count -eq 0) {
        throw "The staged Tesseract resource is empty."
    }
    $lines | Set-Content -LiteralPath $checksumPath -Encoding ascii
}

function Resolve-TesseractDirectory {
    param([string]$RequestedPath)

    $candidates = @()
    if ($RequestedPath) {
        $candidates += $RequestedPath
    }

    $onPath = Get-Command "tesseract.exe" -ErrorAction SilentlyContinue
    if ($onPath) {
        $candidates += $onPath.Source
    }

    $candidates += @(
        "C:\Program Files\Tesseract-OCR",
        "C:\Program Files (x86)\Tesseract-OCR"
    )

    foreach ($candidate in $candidates) {
        if (-not $candidate) {
            continue
        }
        if (Test-Path $candidate -PathType Leaf) {
            $candidate = Split-Path -Parent $candidate
        }
        if (Test-Path (Join-Path $candidate "tesseract.exe") -PathType Leaf) {
            return (Resolve-Path $candidate).Path
        }
    }

    throw @"
Tesseract 5 was not found. Install it, then rerun this script.
For a custom installation, pass -TesseractPath C:\path\to\Tesseract-OCR.
"@
}

Push-Location $ProjectRoot
try {
    $npm = Resolve-RequiredCommand "npm.cmd"
    $node = Resolve-RequiredCommand "node.exe"
    $cargo = Resolve-RequiredCommand "cargo.exe"

    Invoke-Checked $cargo @("--version") "Check Rust"

    if (-not $SkipInstall) {
        Invoke-Checked $npm @("ci") "Install frontend dependencies"
    }

    if (-not $SkipTests) {
        Invoke-Checked $node @("tools/version_manager.mjs", "check") "Check version contract"
        Invoke-Checked $node @("tools/package_audit.mjs", "source") "Reject removed runtime source"
        Invoke-Checked $npm @("test") "Run Node and interface tests"
        Invoke-Checked $npm @("run", "build") "Build the interface and verify final source"
        Invoke-Checked $cargo @("test", "--manifest-path", "src-tauri/Cargo.toml", "--test", "rfi_log_native") "Test the Windows-critical RFI workbook path"
        Invoke-Checked $cargo @("test", "--manifest-path", "src-tauri/Cargo.toml", "--lib", "cap_fs::tests::windows_cross_parent_rename_is_capability_relative_and_no_clobber", "--", "--exact", "--nocapture") "Test Windows cross-parent capability rename"
        Invoke-Checked $cargo @("fmt", "--manifest-path", "src-tauri/Cargo.toml", "--", "--check") "Check Rust formatting"
        Invoke-Checked $cargo @("clippy", "--manifest-path", "src-tauri/Cargo.toml", "--all-targets", "--all-features", "--", "-D", "warnings") "Lint the Rust backend"
        Invoke-Checked $cargo @("test", "--manifest-path", "src-tauri/Cargo.toml", "--no-fail-fast") "Test the Rust backend"
        Invoke-Checked $cargo @("check", "--manifest-path", "src-tauri/Cargo.toml") "Check the Rust backend"
    }

    $tesseractDir = Resolve-TesseractDirectory $TesseractPath
    Write-Host "`n==> Stage Tesseract from $tesseractDir"
    New-Item -ItemType Directory -Force -Path $ResourceDir | Out-Null
    Get-ChildItem $ResourceDir -Force |
        Where-Object { $_.Name -ne ".gitkeep" } |
        Remove-Item -Recurse -Force
    Copy-Item (Join-Path $tesseractDir "*") $ResourceDir -Recurse -Force

    $bundledTesseract = Join-Path $ResourceDir "tesseract.exe"
    $englishData = Join-Path $ResourceDir "tessdata\eng.traineddata"
    if (-not (Test-Path $bundledTesseract -PathType Leaf)) {
        throw "The staged tesseract.exe is missing."
    }
    if (-not (Test-Path $englishData -PathType Leaf)) {
        throw "The staged English OCR data is missing."
    }
    Assert-TesseractResourceEvidence $ResourceDir
    Write-ResourceChecksums $ResourceDir
    Invoke-Checked $node @("tools/license_audit.mjs", "--target", "x86_64-pc-windows-msvc") "Audit locked dependency and OCR licenses"
    Invoke-Checked $npm @("run", "tauri", "build", "--", "--bundles", "nsis") "Build the Windows installer"
    $application = "src-tauri\target\release\docflow.exe"
    Assert-WindowsGuiExecutable $application

    $smokeReport = Join-Path ([System.IO.Path]::GetTempPath()) "docflow-package-smoke-$([guid]::NewGuid()).json"
    try {
        Write-Host "`n==> Run Rust packaged bootstrap/settings/OCR/apply/undo smoke"
        $smoke = Start-Process -FilePath (Resolve-Path $application).Path -ArgumentList @(
            "--package-smoke",
            "`"$smokeReport`"",
            "--tesseract",
            "`"$((Resolve-Path $bundledTesseract).Path)`""
        ) -Wait -PassThru
        if ($smoke.ExitCode -ne 0) {
            throw "Rust packaged smoke failed with exit code $($smoke.ExitCode)."
        }
        if (-not (Test-Path -LiteralPath $smokeReport -PathType Leaf)) {
            throw "Rust packaged smoke exited without producing its report."
        }
        Get-Content -LiteralPath $smokeReport
    }
    finally {
        Remove-Item -LiteralPath $smokeReport -Force -ErrorAction SilentlyContinue
    }

    $installer = Get-ChildItem "src-tauri\target\release\bundle\nsis\*.exe" |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $installer) {
        throw "The NSIS installer was not produced."
    }
    Invoke-Checked $node @("tools/package_audit.mjs", "package", $installer.FullName) "Audit the NSIS artifact name and linked payload"
    Invoke-Checked $node @("tools/checksum.mjs", $installer.FullName) "Create the NSIS SHA-256 checksum"

    Write-Host "`nWindows installer ready:"
    Write-Host $installer.FullName -ForegroundColor Green
    Write-Host "$($installer.FullName).sha256" -ForegroundColor Green
}
finally {
    Pop-Location
}
