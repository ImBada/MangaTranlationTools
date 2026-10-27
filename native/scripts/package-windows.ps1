# Windows release (Electron build: nsis, zip). Writes the zip; builds the installer when
# NSIS (makensis) is on PATH.
# Usage: powershell -File native/scripts/package-windows.ps1 [-OutDir <dir>]
param([string]$OutDir = "")
$ErrorActionPreference = "Stop"

$NativeDir = Resolve-Path (Join-Path $PSScriptRoot "..")
$RepoDir = Resolve-Path (Join-Path $NativeDir "..")
if (-not $OutDir) { $OutDir = Join-Path $NativeDir "target\bundle" }
$Version = (Select-String -Path (Join-Path $NativeDir "Cargo.toml") -Pattern '^version = "(.*)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$Name = "MangaTranslationTools"

cargo build --release -p mtt-app --manifest-path (Join-Path $NativeDir "Cargo.toml")

$Stage = Join-Path $OutDir $Name
if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
New-Item -ItemType Directory -Force -Path $Stage | Out-Null
Copy-Item (Join-Path $NativeDir "target\release\manga-translation-tools.exe") (Join-Path $Stage "$Name.exe")
$Icon = Join-Path $RepoDir "build\icon.ico"
if (Test-Path $Icon) { Copy-Item $Icon (Join-Path $Stage "icon.ico") }

$Zip = Join-Path $OutDir "$Name-$Version-win.zip"
if (Test-Path $Zip) { Remove-Item -Force $Zip }
Compress-Archive -Path (Join-Path $Stage "*") -DestinationPath $Zip
Write-Output $Zip

if (Get-Command makensis -ErrorAction SilentlyContinue) {
  $Script = Join-Path $OutDir "installer.nsi"
  $Setup = Join-Path $OutDir "$Name Setup $Version.exe"
  @"
Unicode true
Name "$Name"
OutFile "$Setup"
InstallDir "`$LOCALAPPDATA\Programs\$Name"
RequestExecutionLevel user
Section
  SetOutPath "`$INSTDIR"
  File /r "$Stage\*"
  CreateShortcut "`$SMPROGRAMS\$Name.lnk" "`$INSTDIR\$Name.exe"
  CreateShortcut "`$DESKTOP\$Name.lnk" "`$INSTDIR\$Name.exe"
  WriteUninstaller "`$INSTDIR\Uninstall.exe"
SectionEnd
Section "Uninstall"
  Delete "`$SMPROGRAMS\$Name.lnk"
  Delete "`$DESKTOP\$Name.lnk"
  RMDir /r "`$INSTDIR"
SectionEnd
"@ | Set-Content -Encoding UTF8 $Script
  makensis $Script | Out-Null
  Write-Output $Setup
}
