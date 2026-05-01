export const FORCE_INCOMPLETE_LAMA_NOTICE = false;

export type LamaNoticePlatform = "darwin" | "win32" | "linux";

export const LAMA_TEST_PLATFORM_OPTIONS: { label: string; value: LamaNoticePlatform }[] = [
  { label: "macOS", value: "darwin" },
  { label: "Windows", value: "win32" },
  { label: "Linux", value: "linux" }
];

export const LAMA_TEST_INSTALL_GUIDE: Record<LamaNoticePlatform, { command: string; help: string[] }> = {
  darwin: {
    command: "brew install python@3.11",
    help: [
      "Homebrew가 있으면 터미널에서 위 명령을 실행하세요.",
      "Homebrew가 없으면 https://www.python.org/downloads/macos/ 에서 Python 3.11 이상 macOS installer를 설치하세요.",
      "설치 후 터미널에서 `python3 --version`이 동작하는지 확인한 뒤 앱에서 새로고침을 누르세요."
    ]
  },
  win32: {
    command: "winget install Python.Python.3.11",
    help: [
      "Windows 터미널에서 위 명령을 실행하세요.",
      "winget을 쓸 수 없으면 https://www.python.org/downloads/windows/ 에서 Python 3.11 이상 installer를 받고, 설치 중 Add python.exe to PATH를 켜세요.",
      "설치 후 새 터미널에서 `py -3.11 --version` 또는 `python --version`을 확인한 뒤 앱에서 새로고침을 누르세요."
    ]
  },
  linux: {
    command: "sudo apt-get update && sudo apt-get install -y python3.11 python3.11-venv",
    help: [
      "Debian/Ubuntu 계열은 위 명령을 실행하세요.",
      "다른 배포판은 패키지 매니저로 Python 3.11 이상과 venv 모듈을 설치하세요.",
      "설치 후 `python3 --version`이 동작하는지 확인한 뒤 앱에서 새로고침을 누르세요."
    ]
  }
};
