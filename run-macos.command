#!/bin/bash
# Double-click in Finder: builds the native app and launches it.
# The first build takes a few minutes; later builds only recompile what changed.

cd "$(dirname "$0")" || exit 1

pause_and_exit() {
  echo
  read -r -n 1 -p "아무 키나 누르면 창이 닫힙니다." _
  exit "$1"
}

# Finder-launched shells may not have rustup's PATH.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
export PATH="$HOME/.cargo/bin:$PATH"
if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust(cargo)가 설치되어 있지 않습니다."
  echo "터미널에서 아래 명령으로 설치한 뒤 다시 실행하세요:"
  echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
  pause_and_exit 1
fi

APP_NAME="MangaTranslationTools"
APP_PATH="$PWD/target/bundle/$APP_NAME.app"

# Only one instance runs per data folder: a running old build would just be brought to
# the front, so offer to quit it before rebuilding.
if pgrep -f "$APP_NAME.app/Contents/MacOS/$APP_NAME" >/dev/null 2>&1; then
  echo "$APP_NAME 이(가) 이미 실행 중입니다."
  read -r -p "종료하고 새로 빌드해서 실행할까요? 변경 내용은 저장됩니다. [y/N] " answer
  if [[ ! "$answer" =~ ^[Yy]$ ]]; then
    open "$APP_PATH" 2>/dev/null
    echo "실행 중인 앱을 앞으로 가져왔습니다."
    exit 0
  fi
  osascript -e "tell application \"$APP_NAME\" to quit" >/dev/null 2>&1
  for _ in $(seq 1 20); do
    pgrep -f "$APP_NAME.app/Contents/MacOS/$APP_NAME" >/dev/null 2>&1 || break
    sleep 0.5
  done
fi

echo "빌드 중입니다... (처음에는 몇 분 걸립니다)"
if ! ./scripts/bundle-macos.sh; then
  echo
  echo "빌드에 실패했습니다. 위 오류 메시지를 확인하세요."
  pause_and_exit 1
fi

open "$APP_PATH"
echo
echo "실행했습니다. 이 터미널 창은 닫아도 됩니다."
