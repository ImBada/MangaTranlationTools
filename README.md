# MangaTranslationTools

만화 이미지를 브라우저에서 불러와 한국어로 번역하고, 번역된 문장을 식질할 수 있는 웹앱입니다.

## 주요 기능

- 보관함: 이미지, 폴더, 압축파일, 압축파일 폴더 구조를 작품/화 단위로 가져오고 이어서 작업할 수 있습니다.
- AI 번역: OpenAI API 또는 OpenAI 호환 `/v1/chat/completions` 엔드포인트를 사용하며, 빠름/정확성 모드와 병렬 번역을 설정할 수 있습니다.
- 진행 관리: 페이지별 작업 완료 체크를 지원하며, 완료된 페이지는 이후 일괄 번역과 전체 인페인트 대상에서 제외됩니다.
- 식질 편집: 번역 블록 생성, 삭제, 복제, 다중 선택, 범위 선택, 인라인 텍스트 수정, 찾아바꾸기, 텍스트 선택 분할 복제를 지원합니다.
- 폰트 도구: 시스템 폰트 선택, 굵기/기울임/밑줄, 폰트 크기 프리셋, 블록 타입별 폰트 프리셋, 즐겨찾기 태그, 프리셋 백업/복원을 지원합니다.
- 레이어 편집: 원본, 인페인트, 인페인트 결과, 인페인트 마스크, 번역 오버레이, 최종 출력 레이어를 따로 보고 편집할 수 있습니다.
- 인페인트/PSD: 마스크와 결과 레이어를 분리 저장하고, 부분 재인페인트, 결과 레이어 보정, PSD 내보내기/가져오기를 지원합니다.
- 출력: 현재 페이지 또는 전체 페이지를 최종 PNG로 저장할 수 있습니다.

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
7. 번역이 끝나면 페이지를 열어 텍스트, 폰트, 위치, 인페인트 결과를 수정합니다.
8. 검수가 끝난 페이지는 작업 완료로 표시하고, 필요한 페이지나 전체 페이지를 출력합니다.

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

- 한손모드: 편집 화면에서 `Q` 키를 삭제 키처럼 사용
- 번역 모드: `빠름`, `정확성`
- 번역 엔진: `OpenAI API`, `커스텀`
- Codex 모델: 기본값 `gpt-5.5`
- openai-oauth 포트: 기본값 `10531`
- 커스텀 OpenAI 호환 엔드포인트: 기본값 `http://127.0.0.1:11434/v1`
- 커스텀 모델: 사용할 OpenAI 호환 모델명
- 생각 수준: `없음`, `낮음`, `보통`, `높음`, `최고`
- AI 번역 병렬 처리: 최대 동시 번역 수 `1`~`8`
- `NSFW 모드`: 성인향 이미지 번역을 허용하는 설정
- 모델 테스트: 설정한 엔드포인트가 간단한 텍스트 요청에 응답하는지 확인

## 편집 단축키

- `1`~`5`: 출력, 번역 오버레이, 인페인트 결과, 인페인트 마스크, 원본 이미지 레이어 선택
- `3`/`4`: 인페인트 결과 또는 마스크 레이어가 선택된 상태에서 한 번 더 누르면 인페인트 그룹 레이어로 이동
- `A`: 일반 마우스, `T`: 범위 선택, `Z`: 줌 도구(드래그 오른쪽 확대, 왼쪽 축소), `Space`: 임시 패닝
- `D`/`F` 또는 방향키: 이전/다음 페이지 이동
- `E`: 선택한 번역 블록 인라인 수정
- `Ctrl/Cmd+F`: 찾아바꾸기
- `Ctrl/Cmd+C`: 선택 블록 복사
- `Ctrl/Cmd+V`: 복사한 폰트 설정이 있으면 선택 블록에 적용하고, 없으면 블록 붙여넣기
- `Ctrl/Cmd+클릭`: 번역 블록 복제
- `Shift+클릭`/`Shift+드래그`: 블록 다중 선택 토글
- `Alt+드래그`: 선택 블록의 렌더 범위 변경
- `Shift+드래그`: 선택 블록 이동 중 세로 위치 고정
- `Delete`/`Backspace`: 선택 블록 또는 선택 범위 삭제
- `₩`, `` ` ``, `\`: 현재 페이지 작업 완료 토글
- 인페인트 마스크/결과 레이어에서 `B`: 브러시, `E`: 지우개, `Alt+E`: 자동 지우개
- `I`: 인페인트 결과 레이어 색상 추출. 다른 레이어에서는 `3 인페인트 결과` 레이어를 표시/선택한 뒤 색상 추출 도구 선택

## LaMa 인페인트

기본 인페인트는 앱 내장 `local-fill-fallback`으로 동작합니다. LaMa용 Python 환경과 Er0mangaInpaint 코드는 `~/Documents/MangaTranslationTools/tools/` 아래에 준비됩니다.

인페인트 마스크와 결과 레이어는 별도로 저장됩니다. 마스크 레이어에서는 브러시, 지우개, 자동 지우개, 선택 범위 채우기/비우기, 선택 범위 재인페인트를 사용할 수 있습니다. 결과 레이어에서는 브러시, 지우개, 흐림, 선명, 뭉개기, 마스크 유지 재인페인트, 선택 범위 재인페인트를 사용할 수 있습니다.

PSD 내보내기/가져오기는 인페인트 도구의 PSD 섹션에서 사용할 수 있습니다. PSD 내보내기는 원본, 인페인트 마스크, 인페인트 결과, 번역 블록 레이어를 포함합니다. PSD를 다시 가져오면 인페인트 마스크와 결과 레이어를 현재 페이지에 반영합니다.

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

개발/CI 기준 Node.js 버전은 22입니다.

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
- `MANGA_TRANSLATOR_CODEX_REASONING_EFFORT`: Codex 사용 시 기본 생각 수준
- `MANGA_TRANSLATOR_CODEX_OAUTH_PORT`: Codex OAuth 엔드포인트 포트
- `MANGA_TRANSLATOR_OPENAI_COMPATIBLE_BASE_URL`: OpenAI 호환 API 주소
- `MANGA_TRANSLATOR_OPENAI_COMPATIBLE_API_KEY`: OpenAI 호환 API 키
- `MANGA_TRANSLATOR_OPENAI_COMPATIBLE_MODEL`: OpenAI 호환 API 모델명
- `MANGA_TRANSLATOR_PARALLEL_ENABLED`: AI 번역 병렬 처리 기본값
- `MANGA_TRANSLATOR_PARALLEL_MAX_CONCURRENCY`: AI 번역 최대 동시 처리 수
- `MANGA_TRANSLATOR_PAGE_RETRIES`: 페이지별 번역 재시도 횟수
- `MANGA_TRANSLATOR_TEMPERATURE`: 번역 요청 temperature
- `MANGA_TRANSLATOR_TOP_P`: 번역 요청 top_p
- `MANGA_TRANSLATOR_MAX_TOKENS`: 번역 응답 토큰 예산
- `MANGA_TRANSLATOR_IMAGE_MIN_TOKENS`: 이미지 입력 최소 토큰 예산
- `MANGA_TRANSLATOR_IMAGE_MAX_TOKENS`: 이미지 입력 최대 토큰 예산
- `MANGA_TRANSLATOR_LAMA_COMMAND`: LaMa 인페인트 실행 명령
- `MANGA_TRANSLATOR_LAMA_ARGS`: LaMa 인페인트 실행 인자 JSON
- `MANGA_TRANSLATOR_LAMA_CODE_DIR`: LaMa 인페인트 코드 위치
- `MANGA_TRANSLATOR_LAMA_WEIGHTS`: LaMa 모델 파일 위치

## 라이선스

이 프로젝트는 **GNU General Public License v3.0 (or later)** 하에 배포됩니다. 자세한 내용은 [LICENSE](LICENSE) 파일을 참고하세요.

## 테스트

```powershell
npm run audit -- --omit=dev
npm run format:check
npm run lint
npm run typecheck
npm test
npm run test:e2e
```

CI에서는 프로덕션 의존성 audit, 포맷 검사, lint, typecheck, 빌드, Vitest, Electron 패키징 스모크 테스트를 실행합니다. Electron E2E는 `main`/`master` push, 매일 03:00 KST nightly, 수동 실행, 또는 `run-e2e` 라벨이 붙은 PR에서 실행됩니다.

## 참고 프로젝트

- [Gemma4MangaTranslatorForKorean](https://github.com/ucx0204/Gemma4MangaTranslatorForKorean): 기초 AI 번역 프롬프트 및 시스템 기반 참고
- [koharu](https://github.com/mayocream/koharu): 인페인트 엔진 참고
