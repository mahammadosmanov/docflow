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
    $cargo = Resolve-RequiredCommand "cargo.exe"

    $pythonCommand = Get-Command "python.exe" -ErrorAction SilentlyContinue
    $pythonPrefix = @()
    if ($pythonCommand) {
        $python = $pythonCommand.Source
    } else {
        $python = Resolve-RequiredCommand "py.exe"
        $pythonPrefix = @("-3.12")
    }

    Invoke-Checked $python ($pythonPrefix + @("-c", "import sys; assert sys.version_info >= (3, 12), sys.version")) "Check Python 3.12+"
    Invoke-Checked $cargo @("--version") "Check Rust"

    if (-not $SkipInstall) {
        Invoke-Checked $python ($pythonPrefix + @("-m", "pip", "install", "--upgrade", "pip")) "Update pip"
        Invoke-Checked $python ($pythonPrefix + @("-m", "pip", "install", "-r", "requirements-dev.txt")) "Install Python dependencies"
        Invoke-Checked $python ($pythonPrefix + @("-m", "pip", "install", "-e", ".")) "Install DocFlow engine"
        Invoke-Checked $npm @("ci") "Install frontend dependencies"
    }

    if (-not $SkipTests) {
        Invoke-Checked $python ($pythonPrefix + @("-m", "pytest")) "Run Python tests"
        Invoke-Checked $python ($pythonPrefix + @("-m", "mypy", "src/docflow")) "Run Python type checks"
        Invoke-Checked $npm @("test") "Run interface tests"
        Invoke-Checked $npm @("run", "build") "Build the interface"
        Invoke-Checked $cargo @("check", "--manifest-path", "src-tauri/Cargo.toml") "Check the Tauri shell"
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

    Invoke-Checked $python ($pythonPrefix + @("tools/build_sidecar.py")) "Build the Python sidecar"
    $sidecar = Get-ChildItem "src-tauri\binaries\docflow-sidecar-*.exe" |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $sidecar) {
        throw "The Windows sidecar was not produced."
    }

    Invoke-Checked $sidecar.FullName @("--ping") "Smoke-test the sidecar"
    $env:DOCFLOW_TESSERACT = (Resolve-Path $bundledTesseract).Path
    Invoke-Checked $python ($pythonPrefix + @("tools/ci_ocr_smoke.py", $sidecar.FullName)) "Run packaged OCR smoke test"
    Invoke-Checked $npm @("run", "tauri", "build", "--", "--bundles", "nsis") "Build the Windows installer"

    $installer = Get-ChildItem "src-tauri\target\release\bundle\nsis\*.exe" |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if (-not $installer) {
        throw "The NSIS installer was not produced."
    }

    Write-Host "`nWindows installer ready:"
    Write-Host $installer.FullName -ForegroundColor Green
}
finally {
    Pop-Location
}
