# Build the portable Windows bundle: dist/Specimen-portable-win64.zip
# Usage: powershell -File scripts/package-portable.ps1
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

cargo build --release --bins
if ($LASTEXITCODE -ne 0) { throw "release build failed ($LASTEXITCODE)" }

$stage = Join-Path $root 'dist\stage\Specimen-portable-win64'
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Path (Join-Path $stage 'outputs') -Force | Out-Null

Copy-Item target\release\specimen.exe $stage
Copy-Item target\release\hamesh.exe $stage
Copy-Item outputs\after-frequency.specimen.json (Join-Path $stage 'outputs')
Copy-Item README.md $stage
# Re-create the launcher so the bundle is fully reproducible from this script.
$launcher = @'
@echo off
rem Specimen portable launcher. Projects and evidence stay beside this program.
cd /d "%~dp0"
if exist "outputs\after-frequency.specimen.json" (
  start "" "specimen.exe" --project=outputs\after-frequency.specimen.json
) else (
  start "" "specimen.exe"
)
'@
[IO.File]::WriteAllText((Join-Path $stage 'Launch Specimen.cmd'), ($launcher -replace "`r`n", "`n" -replace "`n", "`r`n"), [Text.UTF8Encoding]::new($false))

$zip = Join-Path $root 'dist\Specimen-portable-win64.zip'
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
Write-Output "Built $zip ({0:N2} MB)" -f ((Get-Item $zip).Length / 1MB)
