# MangaTranslationTools

만화 이미지를 불러와 한국어로 번역하고, 번역된 문장을 식질해 이미지로 출력하는 데스크톱 앱입니다.
Rust + [egui](https://github.com/emilk/egui)로 만든 네이티브 앱이며 macOS · Windows · Linux에서 돌아갑니다.
(예전 웹/Electron 버전은 `v0.9.31`까지이며, 같은 보관함 데이터를 그대로 읽고 씁니다.)

## 주요 기능

- 보관함: 이미지, 폴더, 압축파일(ZIP·CBZ·RAR·CBR), PDF(쪽마다 300DPI 이미지), 그런 파일이 든 폴더 구조를 작품/화 단위로 가져오고 이어서 작업합니다. 창에 끌어다 놓아도 됩니다.
- AI 번역: ChatGPT(Codex) 계정 또는 OpenAI 호환 `/v1/chat/completions` 엔드포인트(ollama 등 로컬 모델 포함)를 쓰며, 빠르게/정확하게 모드와 병렬 번역을 설정합니다.
  번역한 블록은 원문 글자색(흰 글자면 상자 색도)과 그림 위 글자의 테두리 외곽선을 따라갑니다.
- 진행 관리: 페이지별 검수 완료 표시. 검수 완료한 페이지는 이후 일괄 번역과 모든 페이지 인페인트에서 건너뜁니다.
  다시 번역해도 직접 고치거나 만든 블록(문장·위치·배치·프리셋)은 그대로 두고, 그 자리에 새로 나온 블록은 버립니다.
- 식질 편집: 블록 만들기·삭제·복제, 여러 개 선택, 범위 선택, 캔버스에서 바로 고치기, 찾아 바꾸기, 블록 그룹(그룹 그림자).
  번역할 때 블록이 말풍선 안에 있으면 말풍선을 찾아 그 안쪽을 렌더 범위로 쓰고, 가로쓰기 가운데 정렬 줄은 줄마다 폭을 말풍선 윤곽에 맞춥니다(원래 박스는 원문 위치로 남아 인페인트에 쓰임). 붙은 말풍선이나 한 말풍선에 든 여러 블록은 가장 좁은 목에서, 목이 없으면 블록 사이 가운데에서 나눠 갖습니다. 윤곽은 블록의 `배치 › 말풍선 › 윤곽 해제`로 뗄 수 있습니다.
  새로 만든 블록은 줄 길이를 고르게 나누고 문장부호 뒤에서 줄을 바꾸며, 자동 맞춤은 원문 글자 크기의 1.2배까지만 키우고 9px 아래로는 줄이지 않습니다.
  예전 블록은 모양이 그대로이고, 블록의 `배치 › 조판 › 새 규칙`으로 바꿀 수 있습니다.
- 글꼴: 시스템 글꼴, 굵기/기울임/밑줄, 크기 프리셋, 블록 종류별 글꼴 프리셋, 즐겨찾기, 글자별 글꼴, 프리셋 백업/복원.
- 인페인트: 로컬 채우기 또는 LaMa로 원문 글자(어두운 상자의 밝은 글자 포함)를 지우고, 마스크·결과 레이어를 브러시/지우개/스마트 브러시/흐림·선명·뭉개기로 다듬습니다. PSD 내보내기/가져오기(내보낸 PSD에는 블록마다 편집 가능한 텍스트 레이어가 숨긴 그룹 `번역 텍스트 (편집용)`으로 들어감).
- 출력: 현재 페이지 또는 전체 페이지를 최종 PNG로 저장하고(전체는 다운로드 폴더에 CBZ로도 묶을 수 있음), 블록별 원문·결과를 비교하는 HTML 보고서를 만듭니다.

## 설치 / 실행

- **배포판**: [Releases](https://github.com/ImBada/MangaTranlationTools/releases)에서 macOS(dmg, zip) 또는 Windows(설치 파일, zip)를 받습니다.
- **소스에서 바로 실행 (더블클릭)**: Rust가 없다면 먼저 <https://rustup.rs>에서 설치하세요.
  - macOS: Finder에서 `run-macos.command`를 더블클릭합니다. 터미널 창이 열리고 앱을 빌드한 뒤(처음엔 몇 분, 이후엔 바뀐 부분만) 실행합니다.
    앱이 이미 실행 중이면 종료하고 새로 빌드할지 묻습니다(변경 내용은 저장됨). 실행된 뒤에는 터미널 창을 닫아도 됩니다.
    만들어진 앱은 `target/bundle/MangaTranslationTools.app`이며, 직접 더블클릭하거나 응용 프로그램 폴더로 옮겨 써도 됩니다.
  - Windows: 탐색기에서 `run-windows.bat`을 더블클릭합니다.

## 사용 순서

1. 보관함에서 `가져오기 ▾`(이미지 · 폴더 · 압축파일 · PDF · 여러 화 일괄 번역)를 누르거나 파일을 창에 끌어다 놓습니다.
2. 새 작품을 만들지 기존 작품에 넣을지 고르고, 화 제목을 확인해 가져옵니다. 가져오면 AI 번역이 바로 시작됩니다.
3. **번역** 모드(`2`)에서 블록을 눌러 문장, 프리셋, 배치, 스타일을 고칩니다. `Tab`으로 다음 블록, `D`/`F`로 페이지를 넘깁니다.
   모델이 놓친 글자는 `T`로 박스를 그린 뒤 블록의 `다시 번역`을 누르면 그 박스 안 원문만 읽어 번역합니다(위치·스타일은 그대로).
4. **인페인트** 모드(`3` 결과 / `4` 마스크)에서 `이 페이지 인페인트` 또는 `모든 페이지`로 원문 글자를 지우고, 남은 자국을 다듬습니다.
5. **검수 · 출력** 모드(`1`)에서 최종 결과를 넘겨 보며 `₩`로 검수 완료 표시(다음 검수할 페이지로 넘어감)를 하고, 페이지나 전체를 출력합니다.

`Space`를 누르고 있으면 어느 모드에서든 최종 결과를 미리 보고 끌어서 화면을 옮깁니다. 단축키 전체는 `?`로 봅니다.

## 화면 구성

- **보관함**: 왼쪽 작품 목록, 오른쪽 선택한 작품의 화 목록(번역 상태, 검수한 페이지 수). 화를 누르면 열리고, 끌어서 순서를 바꾸고, 우클릭으로 이름 변경·삭제합니다.
- **편집 화면**
  - 상단 바: `← 보관함`, 작품 › 화 메뉴(다른 화 열기, 새로 가져오기), 모드 탭, `AI 번역`(번역 안 된 페이지 이어서, `▾`로 전체 다시/이 페이지만), 설정
  - 모드 탭: **번역**(`2`) · **인페인트**(결과 `3` / 마스크 `4`) · **검수 · 출력**(`1`). `5`는 원본 이미지 보기를 켜고 끕니다.
  - 왼쪽 페이지 목록: 썸네일, 번역·인페인트 상태, 오른쪽 동그라미로 검수 완료 표시. 우클릭으로 다시 번역·삭제, 끌어서 순서 변경.
  - 캔버스 위 툴바: 모드별 도구(아이콘, 마우스를 올리면 이름과 단축키), `원본`, `보기 ▾`(레이어 표시·투명도, 위젯), 배율.
  - 오른쪽 인스펙터: 번역 모드는 블록 목록 + 선택한 블록(블록 → 프리셋 → 배치 → 스타일)과 `프리셋` 탭, 인페인트 모드는 실행·브러시·범위·PSD, 검수 모드는 검수 완료·출력·보고서.
    블록 · 프리셋 · 배치 · 스타일 영역은 제목 왼쪽 `⠿`를 끌어 순서를 바꿉니다.
  - 하단 상태 바: 저장 상태, 페이지 정보, 최근 메시지(누르면 기록), 진행 중인 작업, 단축키 표.
  - 위젯: **페이지**, **텍스트 블록**, **블록**, **프리셋**, **이 페이지**는 머리글 오른쪽 `⇱`로 따로 창으로 띄워 원하는 곳에 두고 `⇲`로 다시 붙입니다
    (`보기 ▾`에서도 켜고 끔, `위젯 배치 초기화`로 처음 상태로). 창 위치는 다음 실행에도 그대로이고, 앱 창 크기가 바뀌면 가까운 캔버스 가장자리와의 거리를 유지합니다.
  - 단축키가 있는 버튼에는 옆에 작은 글자로 키가 붙어 있고, 마우스를 올리면 이름과 함께 다시 보입니다. 아이콘은 [Phosphor Icons](https://phosphoricons.com)(MIT) 글꼴입니다.

## 저장 위치

- 데이터 폴더: `~/Documents/MangaTranslationTools/` (`MANGA_TRANSLATOR_DATA_DIR`로 변경)
  - 보관함 `library/`, 설정 `settings.json`, ChatGPT 로그인 `codex-auth.json`, 로그 `logs/native-app.log`(`MANGA_TRANSLATOR_LOG_PATH`로 변경)
  - 위젯 배치·카드 순서·최근 글꼴 같은 화면 상태 `native-ui-state.json`
  - LaMa 모델 `models/`, LaMa용 LibTorch `tools/libtorch/`
- 웹 버전이 쓰던 `chapter.json`을 처음 덮어쓸 때 `chapter.json.native-backup`을 한 번 만들어 둡니다.
- 같은 데이터 폴더로 두 번 실행하면 이미 열린 창을 앞으로 가져오고 새 프로세스는 끝납니다.

## 설정

- 번역 엔진: `ChatGPT 계정 (Codex)` 또는 `OpenAI 호환 서버`(기본 `http://127.0.0.1:11434/v1`, 모델명 지정)
  - ChatGPT 계정은 설정 › 번역의 `ChatGPT 로그인`으로 앱 안에서 로그인합니다. 브라우저가 열리면 ChatGPT에 로그인하고 앱에 뜬 코드를 입력합니다.
    토큰은 데이터 폴더의 `codex-auth.json`(macOS·Linux는 본인만 읽기 가능)에 두고 만료 전에 저절로 갱신합니다. 기본 모델 `gpt-5.5`, 생각 수준 없음~최고.
- 번역 모드: `빠르게`, `정확하게`
- 추가 지시문: 모든 번역 요청 끝에 붙는 말투·표기 규칙. 작품마다의 인물 이름·용어는 편집 화면 작품 › 화 메뉴의 `번역 메모…`에 적습니다(`work.json`의 `translationNotes`).
- AI 번역 병렬 처리: 최대 동시 번역 수
- 모델 테스트: 설정한 엔드포인트가 간단한 요청에 응답하는지 확인
- 한손모드: 편집 화면에서 `Q`를 삭제 키처럼 사용
- 설정 › 편집 탭에서 단축키 전체를 볼 수 있습니다.

## LaMa 인페인트

기본 인페인트는 앱 안의 로컬 채우기입니다. LaMa를 쓰려면 설정의 **인페인트** 탭에서 `LaMa 받기`를 합니다. 모델(약 200MB)과 모델을 돌릴 LibTorch(PyTorch의 C++ 엔진, 압축 약 70~120MB)를 받습니다.
모델은 앱 안에서 LibTorch로 돌아갑니다(Apple Silicon Mac은 GPU(MPS), Windows·Linux x64는 CPU). Intel Mac에는 LibTorch 빌드가 없어 LaMa를 쓸 수 없습니다.
Windows·Linux x64에서 NVIDIA GPU(CUDA 13.3 이상을 지원하는 드라이버, Turing 이상)가 있으면 인페인트 탭의 `실행 장치`에서 `GPU (CUDA)`를 골라 CUDA용 LibTorch와 NVIDIA 런타임 라이브러리(Windows 약 2.3GB, Linux 약 2.7GB)를 받습니다.
CUDA로 못 돌면 CPU로 돌아가고 이유를 설정에 표시합니다. (CUDA 경로는 아직 실제 NVIDIA 장비에서 확인하지 않았습니다.)
둘레가 한 가지 색인 글자(흰 말풍선 안 등)는 모델 없이 그 색으로 채우고, 나머지만 한 변 1024px 이하 영역으로 나눠 모델에 넘깁니다.
그보다 큰 영역은 줄이지 않고, 주변 128px을 붙인 타일로 나눠 원래 해상도로 차례로 채웁니다.

- 모델: `https://huggingface.co/mayocream/lama-manga/resolve/main/lama-manga.safetensors`를 `~/Documents/MangaTranslationTools/models/lama-manga/lama-manga.safetensors`에 (직접 받아 둬도 됩니다)
- LibTorch: [koharu](https://github.com/mayocream/koharu)가 C 연결 코드와 함께 빌드해 올린 [koharu-rs/torch](https://github.com/koharu-rs/torch/releases) `v2.13.0.7`. 받은 파일은 SHA-256을 확인한 뒤 데이터 폴더의 `tools/libtorch/`에 풉니다(풀면 Mac 약 340MB).

### 글자 분할 모델 (선택)

설정의 **인페인트** 탭에서 `글자 분할 모델 받기`를 하면 블록 글자 마스크를 밝기 규칙 대신 모델(UNet++ / EfficientNetV2, 약 217MB)로 찾습니다.
색 글자, 외곽선 글자, 스크린톤·그림 위 글자에서 더 정확하고, LaMa와 같은 LibTorch로 앱 안에서 돕니다(없으면 함께 받음). 모델이 아무것도 못 찾은 블록은 예전 규칙으로 돌아갑니다.

- 모델: `https://huggingface.co/mayocream/manga-text-segmentation-2025` (리비전 고정, SHA-256 확인). Manga109로 학습되었고 가중치에 라이선스 표기가 없습니다.

다른 LaMa 실행 방식(예: NVIDIA GPU에서 도는 다른 프로그램)을 쓰려면 `MANGA_TRANSLATOR_LAMA_COMMAND`와 `MANGA_TRANSLATOR_LAMA_ARGS`(JSON 배열, `{source}` `{mask}` `{output}`은 임시 PNG 경로로 치환)를 지정합니다. 지정하면 모델 대신 이 명령이 인페인트할 영역 전체를 받습니다.

## 환경 변수

| 변수 | 내용 |
|---|---|
| `MANGA_TRANSLATOR_DATA_DIR` | 데이터 폴더 |
| `MANGA_TRANSLATOR_MODEL_PROVIDER`, `_CODEX_MODEL`, `_CODEX_REASONING_EFFORT` | 설정 기본값 |
| `MANGA_TRANSLATOR_OPENAI_COMPATIBLE_BASE_URL`, `_API_KEY`, `_MODEL` | 설정 기본값 |
| `MANGA_TRANSLATOR_PARALLEL_ENABLED`, `_PARALLEL_MAX_CONCURRENCY` | 설정 기본값 |
| `MANGA_TRANSLATOR_TEMPERATURE`, `_TOP_P`, `_MAX_TOKENS`, `_PAGE_RETRIES` | 번역 요청 |
| `MANGA_TRANSLATOR_DOWNLOADS_DIR` | PSD 내보내기 위치 (기본 `~/Downloads`) |
| `MANGA_TRANSLATOR_LOG_PATH` | 로그 파일 |
| `MANGA_TRANSLATOR_LAMA_COMMAND`, `_LAMA_ARGS` | LaMa 실행 명령 직접 지정 |

## 개발

```bash
cargo run --release -p mtt-app                # 개발 실행
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

| 크레이트 | 내용 |
|---|---|
| `crates/mtt-core` | 데이터 모델(모르는 JSON 필드 보존), 보관함, 설정, 가져오기, 글꼴 프리셋, 블록 그룹 |
| `crates/mtt-render` | 텍스트 레이아웃/렌더링(cosmic-text + tiny-skia), 페이지 합성 |
| `crates/mtt-inpaint` | 로컬 채우기 인페인트, LaMa(LibTorch, 영역 나누기·단색 채우기), 블록 글자 마스크, PSD 읽기/쓰기 |
| `crates/mtt-torch` | 실행 중에 불러오는 LibTorch와 LaMa에 필요한 연산 (작업 공간에서 유일하게 unsafe를 쓰는 FFI) |
| `crates/mtt-translate` | OpenAI 호환 / Responses API 번역, 응답 파서 |
| `crates/mtt-app` | egui 편집기 (앱 아이콘·글꼴은 `assets/`) |

### 패키징 / 배포

```bash
./scripts/bundle-macos.sh                      # macOS 앱 번들: target/bundle/MangaTranslationTools.app
./scripts/bundle-macos.sh --dist               # + dmg, zip
./scripts/package-linux.sh                     # Linux: tar.gz(AppDir), appimagetool/dpkg-deb가 있으면 AppImage/deb
powershell -File scripts/package-windows.ps1   # Windows: zip, makensis가 있으면 설치 파일
scripts/release-version.sh [patch|minor|major|x.y.z]   # 버전 커밋 + vX.Y.Z 태그를 push → Release 워크플로가 배포판을 올림
```

Windows/Linux 패키지는 각 OS에서 빌드해야 합니다(`ring` 등 C 의존성 때문에 macOS에서 교차 빌드 불가).
CI는 Linux · macOS · Windows에서 포맷 검사(Linux), clippy, 테스트를 돌립니다.

### 검증 도구

```bash
cargo run -p mtt-core --example library_compat              # 보관함 왕복 직렬화 검사 (읽기 전용)
cargo run -p mtt-render --example make_test_pages -- DIR 3   # 합성 테스트 페이지 생성
MANGA_TRANSLATOR_DATA_DIR=/tmp/x MTT_AUTOMATION="import:DIR;wait-editor;ready;shot:/tmp/a.png;quit" \
  cargo run --release -p mtt-app                             # UI 자동화 + 스크린샷 (단계 목록은 crates/mtt-app/src/automation.rs)
```

이전 작업 기록과 남은 일은 [TODO.md](TODO.md)에 있습니다.

## 라이선스

이 프로젝트는 **GNU General Public License v3.0 (or later)** 하에 배포됩니다. 자세한 내용은 [LICENSE](LICENSE) 파일을 참고하세요.
PSD 텍스트 레이어 코드(`crates/mtt-inpaint/src/psd_text.rs`)와 글자 분할 모델 구조(`crates/mtt-inpaint/src/text_segmentation_torch.rs`)는 [koharu](https://github.com/koharu-rs/koharu)(MIT OR Apache-2.0, Copyright 2025-2026 Mayo Takanashi and Koharu contributors)에서 옮겨 왔습니다.

## 참고 프로젝트

- [Gemma4MangaTranslatorForKorean](https://github.com/ucx0204/Gemma4MangaTranslatorForKorean): 기초 AI 번역 프롬프트 및 시스템 기반 참고
- [koharu](https://github.com/mayocream/koharu): 인페인트 엔진 참고
