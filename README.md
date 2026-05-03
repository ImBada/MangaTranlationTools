# MangaTranslationTools

만화 이미지를 브라우저에서 불러와 한국어로 번역하고, 번역된 문장을 식질할 수 있는 웹앱입니다.

## 먼저 알아둘 점

기본은 Node 서버와 브라우저 UI로 동작하는 웹앱입니다. 같은 빌드 결과물을 Electron으로 감싼 데스크톱 앱 패키지도 만들 수 있습니다. 번역은 로컬 모델을 실행하지 않고 Codex 혹은 OpenAI 호환 API 엔드포인트로 보냅니다.

로컬 모델은 OpenAI 호환 API 엔드포인트로 셋팅해서 사용하면 됩니다. (ollama 등)

## 사용자용 설치/사용법

1. 서버를 실행합니다.
2. 브라우저에서 표시된 주소를 엽니다.
3. 오른쪽 위 설정에서 모델과 생각 수준을 확인합니다.
4. `이미지 열기`, `폴더 열기`, `압축파일 열기`, `작품 일괄 번역` 중 하나를 선택합니다.
5. 새 작품을 만들지, 기존 작품에 추가할지 선택합니다.
6. 화 제목을 확인한 뒤 보관함에 추가합니다.
7. 번역이 끝나면 페이지를 열어 필요한 부분만 직접 수정합니다.

### 웹 서버로 실행

```powershell
npm install
npm run build
npm start
```

빌드 후에는 Node 서버가 `out/client`의 정적 파일을 직접 제공합니다. 브라우저에서 표시된 주소를 열면 됩니다.

### 데스크톱 앱 빌드

배포용 앱 패키지를 만들려면:

```powershell
npm run app:build
```

패키징 결과물은 `dist-app/`에 생성됩니다.

### 저장 위치

웹 서버와 데스크톱 앱 모두 기본 저장 데이터는 사용자 문서 폴더 아래에 만들어집니다.

- 기본 위치: `~/Documents/MangaTranslationTools/`
- 보관함: `~/Documents/MangaTranslationTools/library/`
- 로그: `~/Documents/MangaTranslationTools/logs/app.log`
- 설정: `~/Documents/MangaTranslationTools/settings.json`

다른 위치를 쓰려면 실행 전에 `MANGA_TRANSLATOR_DATA_DIR` 환경 변수를 지정하세요.

## 설정

- 번역 모드: `빠름`, `정확성`
- OpenAI 모델: 기본값 `gpt-5.5`
- 커스텀 모델: 기본값 `gemma4:31b`
- 생각 수준: `없음`, `낮음`, `보통`, `높음`, `최고`
- `NSFW 모드`: 성인향 이미지 번역을 허용하는 설정

## LaMa 인페인트

기본 인페인트는 앱 내장 `local-fill-fallback`으로 동작합니다. LaMa용 Python 환경과 Er0mangaInpaint 코드는 `~/Documents/MangaTranslationTools/tools/` 아래에 준비됩니다.

모델 파일은 앱 시작 시 자동 다운로드하지 않습니다. 설정의 `LaMa 인페인트 > 모델 다운로드` 버튼으로 받을 수 있으며, 직접 다운로드해서 아래 경로에 저장해도 됩니다.

- 다운로드: `https://huggingface.co/mayocream/lama-manga/resolve/main/lama-manga.safetensors`
- 저장 경로: `~/Documents/MangaTranslationTools/models/lama-manga/lama-manga.safetensors`

웹 개발 환경에서 직접 준비하려면:

```powershell
npm run lama:prepare
```

기존 저장소 내부 `tools/`를 쓰고 싶다면 서버 실행 전에 아래 환경 변수를 지정하세요.

```powershell
./scripts/prepare-lama-manga-inpaint.sh
$env:MANGA_TRANSLATOR_LAMA_COMMAND = "tools/lama-manga-venv/bin/python"
$env:MANGA_TRANSLATOR_LAMA_ARGS = '["scripts/lama-inpaint.py","--input","{source}","--mask","{mask}","--output","{output}","--weights","tools/inpaint-models/mayocream-lama-manga/lama-manga.safetensors"]'
npm run dev
```

macOS/Linux에서는 아래 스크립트로 환경 변수 설정과 개발 서버 실행을 한 번에 할 수 있습니다.

```bash
./scripts/prepare-lama-manga-inpaint.sh
./scripts/use-lama-manga-inpaint.sh
```

`{source}`, `{mask}`, `{output}`은 앱이 만든 임시 PNG 경로로 자동 치환됩니다. `scripts/lama-inpaint.py`는 `mayocream/lama-manga`의 `lama-manga.safetensors`를 직접 로드합니다.

## 개발자용 구조/환경 변수

### 개발 실행

```powershell
npm install
npm run dev
```

기본 주소:

- UI: `http://127.0.0.1:5173`
- API: `http://127.0.0.1:3000`

개발 중 앱 창으로 확인하려면:

```powershell
npm run app:dev
```

### 경로 구조

- 앱 루트: 기본값은 저장소 루트입니다. `MANGA_TRANSLATOR_APP_ROOT`로 지정할 수 있습니다.
- 런타임 파일: `<앱 루트>/out/app-runtime/`
- 도구 파일: `<앱 루트>/tools/`
- 저장 데이터: 기본값은 `~/Documents/MangaTranslationTools/`입니다. `MANGA_TRANSLATOR_DATA_DIR`로 지정할 수 있습니다.

### 주요 환경 변수

- `MANGA_TRANSLATOR_DATA_DIR`: 보관함, 로그, 설정 파일 저장 위치
- `MANGA_TRANSLATOR_APP_ROOT`: 빌드 결과물과 런타임 도구를 찾을 앱 루트
- `MANGA_TRANSLATOR_PORT`: 웹 서버 포트
- `MANGA_TRANSLATOR_MODEL_PROVIDER`: 기본 모델 제공자
- `MANGA_TRANSLATOR_CODEX_MODEL`: Codex 사용 시 기본 모델
- `MANGA_TRANSLATOR_OPENAI_COMPATIBLE_BASE_URL`: OpenAI 호환 API 주소
- `MANGA_TRANSLATOR_OPENAI_COMPATIBLE_API_KEY`: OpenAI 호환 API 키
- `MANGA_TRANSLATOR_OPENAI_COMPATIBLE_MODEL`: OpenAI 호환 API 모델명
- `MANGA_TRANSLATOR_LAMA_COMMAND`: LaMa 인페인트 실행 명령
- `MANGA_TRANSLATOR_LAMA_ARGS`: LaMa 인페인트 실행 인자 JSON
- `MANGA_TRANSLATOR_LAMA_CODE_DIR`: LaMa 인페인트 코드 위치
- `MANGA_TRANSLATOR_LAMA_WEIGHTS`: LaMa 모델 파일 위치

## 라이선스

이 프로젝트는 **GNU General Public License v3.0 (or later)** 하에 배포됩니다. 자세한 내용은 [LICENSE](LICENSE) 파일을 참고하세요.

## 테스트

```powershell
npm run typecheck
npm test
npm run test:e2e
```

CI에서는 Electron E2E가 `main`/`master` push, 매일 03:00 KST nightly, 수동 실행, 또는 `run-e2e` 라벨이 붙은 PR에서 실행됩니다.
