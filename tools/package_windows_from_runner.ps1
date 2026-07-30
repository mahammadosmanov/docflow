param(
  [Parameter(Mandatory = $true)]
  [string]$RunnerBase,

  [Parameter(Mandatory = $true)]
  [string]$OutputPath
)

$ErrorActionPreference = "Stop"
$repo = Split-Path -Parent $PSScriptRoot
$runner = (Resolve-Path -LiteralPath $RunnerBase).Path
$output = [System.IO.Path]::GetFullPath($OutputPath)
$staging = [System.IO.Path]::GetFullPath((Join-Path $repo "staging"))

if (-not $output.StartsWith($staging, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "OutputPath must stay inside the repository staging directory: $output"
}
if (Test-Path -LiteralPath $output) {
  throw "OutputPath already exists: $output"
}

$appSo = Join-Path $repo "app\build\windows\app.so"
$flutterAssets = Join-Path $repo "app\build\flutter_assets"
$sidecarBuild = Join-Path $repo "sidecar\dist\docflow_sidecar"
$sidecarExe = Join-Path $sidecarBuild "docflow_sidecar.exe"
$requiredFiles = @(
  (Join-Path $runner "docflow.exe"),
  (Join-Path $runner "flutter_windows.dll"),
  (Join-Path $runner "data\icudtl.dat"),
  (Join-Path $runner "tesseract\tesseract.exe"),
  $appSo,
  $sidecarExe
)
foreach ($required in $requiredFiles) {
  if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
    throw "Required package input is missing: $required"
  }
}
if (-not (Test-Path -LiteralPath $flutterAssets -PathType Container)) {
  throw "Flutter assets are missing: $flutterAssets"
}

$dataTarget = Join-Path $output "data"
$sidecarTarget = Join-Path $output "sidecar"
New-Item -ItemType Directory -Path $output | Out-Null
New-Item -ItemType Directory -Path $dataTarget | Out-Null
New-Item -ItemType Directory -Path $sidecarTarget | Out-Null

foreach ($name in @(
  "docflow.exe",
  "flutter_windows.dll",
  "msvcp140.dll",
  "vcruntime140.dll",
  "vcruntime140_1.dll"
)) {
  Copy-Item -LiteralPath (Join-Path $runner $name) -Destination $output
}
Copy-Item -LiteralPath (Join-Path $runner "data\icudtl.dat") -Destination $dataTarget
Copy-Item -LiteralPath $appSo -Destination (Join-Path $dataTarget "app.so")
Copy-Item -LiteralPath $flutterAssets `
  -Destination (Join-Path $dataTarget "flutter_assets") -Recurse

# Copy the CONTENTS of PyInstaller's onedir output. Copying the directory itself
# adds an invalid extra level: sidecar\docflow_sidecar\docflow_sidecar.exe.
Get-ChildItem -LiteralPath $sidecarBuild -Force | ForEach-Object {
  Copy-Item -LiteralPath $_.FullName -Destination $sidecarTarget -Recurse
}
Copy-Item -LiteralPath (Join-Path $runner "tesseract") `
  -Destination (Join-Path $output "tesseract") -Recurse

$packagedSidecar = Join-Path $sidecarTarget "docflow_sidecar.exe"
if (-not (Test-Path -LiteralPath $packagedSidecar -PathType Leaf)) {
  throw "Invalid package layout: $packagedSidecar is missing"
}
$pingLine = '{"id":1,"method":"ping","params":{}}' |
  & $packagedSidecar |
  Select-Object -Last 1
$ping = $pingLine | ConvertFrom-Json
if ($ping.result.ok -ne $true) {
  throw "Packaged sidecar ping failed"
}

Write-Output $output
