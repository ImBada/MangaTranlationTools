# MangaTranslationTools — Rust 네이티브 앱

기존 TypeScript/Electron 앱(`../src`)을 Rust + egui로 옮긴 데스크톱 앱입니다. 같은 보관함 폴더를 그대로 읽고 씁니다.

## 실행

```bash
cd native
cargo run --release -p mtt-app            # 개발 실행
./scripts/bundle-macos.sh                  # macOS 앱 번들: target/bundle/MangaTranslationTools.app
./scripts/bundle-macos.sh --dist           # + dmg, zip (기존 앱의 mac 배포 형식)
./scripts/package-linux.sh                 # Linux: tar.gz(AppDir), appimagetool/dpkg-deb가 있으면 AppImage/deb
powershell -File scripts/package-windows.ps1   # Windows: zip, makensis가 있으면 설치 파일
```

Windows/Linux 패키지는 각 OS에서 빌드해야 합니다(`ring` 등 C 의존성 때문에 macOS에서 교차 빌드 불가).

- 데이터 폴더: 기존 앱과 같은 `~/Documents/MangaTranslationTools` (`MANGA_TRANSLATOR_DATA_DIR`로 변경 가능)
- 기존 `chapter.json`을 처음 덮어쓸 때 `chapter.json.native-backup`을 한 번 만들어 둡니다.
- 로그: `<데이터 폴더>/logs/native-app.log` (`MANGA_TRANSLATOR_LOG_PATH`로 변경 가능)
- 같은 데이터 폴더로 두 번 실행하면 이미 열린 창을 앞으로 가져오고 새 프로세스는 끝납니다.
- 최근 글꼴 목록 같은 UI 상태는 `<데이터 폴더>/native-ui-state.json`에 저장합니다.

## 환경 변수 (기존 앱과 같은 이름)

| 변수 | 내용 |
|---|---|
| `MANGA_TRANSLATOR_DATA_DIR` | 데이터 폴더 |
| `MANGA_TRANSLATOR_MODEL_PROVIDER`, `_CODEX_MODEL`, `_CODEX_REASONING_EFFORT`, `_CODEX_OAUTH_PORT` | 설정 기본값 |
| `MANGA_TRANSLATOR_OPENAI_COMPATIBLE_BASE_URL`, `_API_KEY`, `_MODEL` | 설정 기본값 |
| `MANGA_TRANSLATOR_PARALLEL_ENABLED`, `_PARALLEL_MAX_CONCURRENCY` | 설정 기본값 |
| `MANGA_TRANSLATOR_TEMPERATURE`, `_TOP_P`, `_MAX_TOKENS`, `_PAGE_RETRIES` | 번역 요청 |
| `MANGA_TRANSLATOR_DOWNLOADS_DIR` | PSD 내보내기 위치 (기본 `~/Downloads`) |
| `MANGA_TRANSLATOR_LOG_PATH` | 로그 파일 |
| `MANGA_TRANSLATOR_LAMA_COMMAND`, `_LAMA_ARGS` | LaMa 실행 명령 직접 지정 |

## 구조

| 크레이트 | 내용 |
|---|---|
| `mtt-core` | 데이터 모델(모르는 JSON 필드 보존), 보관함, 설정, 가져오기, 폰트 프리셋, 블록 그룹 |
| `mtt-render` | 텍스트 레이아웃/렌더링(cosmic-text + tiny-skia), 페이지 합성 |
| `mtt-inpaint` | 로컬 채우기 인페인트, LaMa 사이드카, 블록 글자 마스크, PSD 읽기/쓰기 |
| `mtt-translate` | OpenAI 호환 / Responses API 번역, 응답 파서 |
| `mtt-app` | egui 편집기 |

## 검증 도구

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p mtt-core --example library_compat              # 보관함 왕복 직렬화 검사 (읽기 전용)
cargo run -p mtt-render --example make_test_pages -- DIR 3   # 합성 테스트 페이지 생성
MANGA_TRANSLATOR_DATA_DIR=/tmp/x MTT_AUTOMATION="import:DIR;wait-editor;ready;shot:/tmp/a.png;quit" \
  cargo run --release -p mtt-app                             # UI 자동화 + 스크린샷
```

## 참고

- ChatGPT(Codex) 계정 번역은 로컬 `openai-oauth` 프록시(`npx openai-oauth`)에 접속합니다.
- LaMa: 설정의 "LaMa 인페인트"에서 환경 준비(파이썬 가상환경 + 패키지 + Er0mangaInpaint)와 모델 다운로드를 합니다. 기존 앱이 이미 준비한 런타임도 그대로 씁니다.
- 결과 보고서는 임시 폴더에 HTML로 저장하고 기본 브라우저로 엽니다. 보고서의 "이동" 버튼은 앱의 로컬 주소(127.0.0.1)로 블록을 찾아갑니다.
- 블록/폰트 설정 복사는 기존 앱과 같은 JSON을 시스템 클립보드에 넣으므로 두 앱 사이에 붙여넣을 수 있습니다.
- 남은 작업은 `TODO.md`를 보세요.
