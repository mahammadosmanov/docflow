param(
  [string]$Flutter = "C:\Users\Mahammad.Osmanov\develop\flutter\bin\flutter.bat"
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$sidecar = Join-Path $repo "sidecar"
$app = Join-Path $repo "app"
$python = Join-Path $sidecar ".venv\Scripts\python.exe"
$spec = Join-Path $sidecar "docflow_sidecar.spec"

if (-not (Test-Path -LiteralPath $python -PathType Leaf)) {
  throw "Sidecar virtual environment is missing: $python"
}
if (-not (Test-Path -LiteralPath $Flutter -PathType Leaf)) {
  throw "Flutter is missing: $Flutter"
}

Push-Location $sidecar
try {
  & $python -m PyInstaller --noconfirm --clean $spec
  if ($LASTEXITCODE -ne 0) {
    throw "Sidecar build failed with exit code $LASTEXITCODE"
  }
}
finally {
  Pop-Location
}

Push-Location $app
try {
  & $Flutter pub get
  if ($LASTEXITCODE -ne 0) {
    throw "flutter pub get failed with exit code $LASTEXITCODE"
  }
  & $Flutter build windows --release
  if ($LASTEXITCODE -ne 0) {
    throw "Flutter Windows build failed with exit code $LASTEXITCODE"
  }
}
finally {
  Pop-Location
}

$exe = Join-Path $app "build\windows\x64\runner\Release\docflow.exe"
if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
  throw "Build completed without the expected executable: $exe"
}

Write-Output $exe
