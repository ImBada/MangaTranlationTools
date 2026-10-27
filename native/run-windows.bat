@echo off
rem Double-click in Explorer: builds the native app and launches it.
chcp 65001 >nul
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust^(cargo^)가 설치되어 있지 않습니다. https://rustup.rs 에서 설치한 뒤 다시 실행하세요.
  pause
  exit /b 1
)

tasklist /fi "imagename eq manga-translation-tools.exe" | find /i "manga-translation-tools.exe" >nul
if not errorlevel 1 (
  echo 앱이 이미 실행 중입니다. 앱을 닫은 뒤 다시 실행하세요.
  pause
  exit /b 1
)

echo 빌드 중입니다... ^(처음에는 몇 분 걸립니다^)
cargo build --release -p mtt-app
if errorlevel 1 (
  echo 빌드에 실패했습니다. 위 오류 메시지를 확인하세요.
  pause
  exit /b 1
)

start "" "target\release\manga-translation-tools.exe"
