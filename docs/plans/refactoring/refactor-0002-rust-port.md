# REFACTOR-0002: 기존 구조와 동작을 보존하는 Rust 포팅

## Purpose

현재 저장소 안에서 Python 애플리케이션과 필요한 OpenCV/NumPy 연산을 Rust로 옮긴다. **기존 폴더의 책임과 설정·자산은 유지하고, Rust 소스는 루트 `src/`에 대응시키는 단일 Cargo 패키지**를 권장한다. 폴더를 먼저 전면 재편하기보다 같은 입력·시간·관찰에서 같은 판단과 입력 계획을 내는지 비교할 수 있게 만든다.

- 상태: `in-loop`, 갱신일 `2026-10-31`. Rust-only 기본 실행과 Python 소스 정리는 사용자 승인된 Step9이며 최종 검증/공개를 진행한다. 전체 동등성·실게임 수용은 Provisional이다. 최초 요청은 계획 작성이었고, 이후 사용자가 feature 작성·설치/포팅·Windows EXE까지의 전체 실행을 위임했다. [PRD-0009](../prd/prd-0009-rust-windows-port.md)가 상위 범위, 각 feature/spec/run이 실제 실행 경계를 소유한다. 설치/설정/영상/Windows/정책/action/Summoner/GUI/CLI·녹화 구현과 전체 회귀, 최종 폴더 배포의 실제 실행·제한한 실게임/성능 검사를 완료했다. [최종 배포 run](../run/run-20261023-09-rust-windows-distribution.md)과 [평가](../evaluation/eval-0031-rust-windows-distribution.md)가 실행파일·검사·측정과 남은 사람 수용/전체 게임 한계를 소유한다. 사람 수용을 자동으로 기록하지 않는다.
- 사용자 확정 목표: **최종 배포에서 Python·OpenCV 모두 제거**. 포팅 개발 중에는 기존 Python/OpenCV 구현을 비교용으로 유지했고, 그 소스/환경 정리는 아래 Step9의 별도 승인 범위다.
- 동일 저장소·Cargo/src·역할별 모듈과 Rust 소유권/자원/오류 처리는 사용자의 표준적 구조 요청과 실행 위임 아래 구체화한다. GUI/코덱의 실질 호환성·의존성 변경은 해당 경계 전 검토한다. “Rust crate 사용”만으로 계산·코덱의 내부 구현까지 Rust라고 간주하지 않는다.
- 출발 자료: [공유 대화](https://chatgpt.com/share/6ad9f733-2cd8-83e8-bed8-3a162bd8b057?no_universal_links=1), [현재 구조](../../project/architecture.md), [개발 지침](../../project/developer-guide.md), [설정 안내](../../../config/config.md), 아래 실제 코드. 공유 대화의 코드 검토 기준 `9a78c2a`보다 현재 로컬 커밋을 우선한다.
- 별도 트랙인 이유: [좌표 변환 REFACTOR-0001](refactor-0001-frame-coordinates.md)은 기존 승인 범위와 남은 검증을 유지한다. [패키지 경계 PRD-0002](../prd/prd-0002-package-boundary-planning.md)는 Python 패키지 소유권에 관한 미승인 초안이므로, 언어·비전 엔진·GUI·배포 전환을 그 승인의 연장으로 취급하지 않는다.

## Source Retirement — 2026-10-31

사용자는 이번 세션 정리, 모든 남은 Python 코드 제거, Rust-only 검증과 리뷰 뒤 commit/push를 명시했다. 이 추가 승인은 Rust-only 기본 실행을 확정하고 아래 초기 단계의 Python 유지/삭제 제외 조건을 **이번 소스 정리 범위에서** 대체한다. 전체 게임·모든 환경의 외관 수용을 대신하지 않는다. 기준 코드는 `cde68bc2eb1285eddcd8a261cdfe411ed4924f76`이며 이후의 승인된 설정/자산·안전·성능 변경은 별도 delta다. 기존 계획/실행 이력은 당시의 판단 근거로 보존한다.

Step 9의 실행 owner는 [소스 정리 run](../run/run-20261031-01-rust-source-retirement.md), parity와 commit 판단 owner는 [merge check](refactor-0002-rust-port-merge-check.md)다. 삭제 전에 기존 비교 결과와 출처/입력 해시를 고정하고, 비교 실행을 Rust fixture reader로 전환한다. 앱/도구/테스트의 Python 실행, 일회성 생성기, 프로젝트 전용 환경을 제거한다. 생성된 Rust 테이블·고지와 비코드 회귀 자료는 보존한다. 코드 복구 기준은 Git 이력이며 현재 checkout에 Python 복사본을 두지 않는다.

전체 동등성은 여전히 `Unknown/Provisional`이다. 소스 대응표·지정 비교/실패 검사가 있다는 사실과 모든 실제 게임/OS/임의 입력의 같은 동작을 증명했다는 주장은 구분한다. 최신 J의 실게임, 정상 처치/획득/협곡 마감, 북쪽10연속, 남쪽 이후 단계 및50ms 목표는 이 정리 작업의 성공으로 완료되지 않는다.

## Refactor Type

- Primary type: `Structural` — 기존 책임과 외부 계약을 보존하는 언어·구현 이전.
- Parity target: `Behavior-preserving`. 영상 계산·UI·OS 연동의 재구현은 동작 변경 위험이 큰 인터페이스 변경이다. 실제 차이는 `Intentional Deltas`에서 분리하고, 의미 변경을 구조 변경으로 숨기지 않는다.
- Compare baseline / Baseline pin: 로컬 `master`, **`cde68bc2eb1285eddcd8a261cdfe411ed4924f76`**. 조사 시작 시 작업 트리는 깨끗했다. 이후 실행 시 다른 코드가 들어오면 새 기준 SHA와 변경 이유를 기록한다.
- 관찰한 환경: 프로젝트 `.venv`의 Python `3.7.9`, OpenCV `4.13.0`, NumPy `1.21.6`. `requirements.txt`의 런타임 의존성은 버전이 고정되지 않았으며, `pyproject.toml`은 Black 설정을 소유한다.
- 현재 조사 범위: `diablo2/` Python 파일 75개, `tests/test_*.py` 39개. 파일 수이며 통과한 테스트 수가 아니다. 실제 설치 패키지 전체 버전·OpenCV 빌드 정보·설정/자산 해시·비교 자료·회귀 결과는 Step 0에서 고정한다.
- Behavioral parity: **`Provisional` for whole-game equivalence**. 최초 계획에서는 정적 구조·계약만 조사했다. 이후 각 feature의 실제 source/stored/fake·Windows 감사/회귀와 승인한 차이를 기록했다. [최종 run](../run/run-20261023-09-rust-windows-distribution.md)과 [평가](../evaluation/eval-0031-rust-windows-distribution.md)가 통합 증거·측정된 성능 차이·미검증 실제 입력/전체 게임 범위·사람 전환 검토를 소유한다. 지정 검사 통과를 전체 처치/획득/완주 또는 동일 속도로 확대하지 않는다.

## Scope

### 유지할 구조와 Rust 대응안

채택한 구조는 **동일 저장소 + Cargo 패키지 하나 + 기존 역할별 모듈**이다. Cargo 패키지는 빌드·의존성 관리 단위이며, 그 안에 라이브러리 target과 앱 실행 target을 둔다. 역할마다 별도 패키지로 나눈 workspace를 만들지 않았다. 다음은 실제 구현 책임을 나타낸 개요이며 빈 폴더 생성을 요구하지 않는다.

```text
d2bot/
├─ Cargo.toml                   # 패키지 d2bot, 라이브러리와 기본 앱
├─ Cargo.lock                   # 앱 의존성 해석 결과를 추적
├─ rust-toolchain.toml          # 검증한 Rust 버전과 필요한 구성요소
├─ src/
│  ├─ main.rs                   # 얇은 실행 진입점
│  ├─ lib.rs                    # 역할별 모듈 공개
│  ├─ app.rs                    # GUI/CLI/명시적 실행 분기
│  ├─ common.rs + common/      # 설정·좌표·캡처·입력·상태·공용 비전
│  ├─ actions.rs + actions/    # 녹화·조합·줍기·방 lifecycle
│  ├─ runs.rs + runs/
│  │  ├─ base.rs
│  │  └─ summoner.rs + summoner/ # 현재 main/관찰/탐색/전투/마감/routes
│  ├─ ui.rs + ui/              # 기존 제어판 대응
│  ├─ core.rs + core/          # 기존 CLI preview 계약
│  └─ tools.rs + tools/        # 관찰·재생과 명시적 감독 probe subcommand
├─ scripts/ + examples/         # 패키징·개발 전용 성능/비교 도구
├─ main.py + diablo2/           # 전환 검증 동안 기존 Python판 유지
├─ requirements.txt + pyproject.toml
├─ config/                     # 기존 JSON·문서 공용, 복제하지 않음
├─ assets/                     # 기존 템플릿·manifest 공용
├─ tests/
│  ├─ test_*.py                # 기존 Python 회귀
│  ├─ rust-port-parity.rs      # Rust 통합 비교 target 예시
│  └─ fixtures/rust-port/      # 필요한 영구 비교 자료·manifest만
├─ docs/                       # 현재 정책·기능·계획 소유권 유지
├─ recordings/                 # 기존 로컬 출력, 소스/fixture 저장소 아님
└─ target/                     # Cargo 산출물, Git 제외
```

`common.rs + common/`는 상위 파일에서 하위 모듈을 선언하는 Rust 표기다. Python의 `__init__.py` 재수출은 모듈 선언과 필요한 `pub use`에 대응시킨다. Rust 내부 경로는 `d2bot::common`처럼 바뀌지만 역할 이름을 유지한다. 모든 Python 파일을 무조건 한 개의 Rust 파일로 번역할 의무는 없다. 초기 대응은 같은 책임을 기준으로 하고, 분할은 실제 코드 크기·의존성·테스트 경계가 필요할 때 한정한다. 이는 [Cargo 표준 배치](https://doc.rust-lang.org/cargo/guide/project-layout.html)와 [라이브러리/실행 target](https://doc.rust-lang.org/cargo/reference/cargo-targets.html)에 맞는다.

| 현재 경계 | Rust 대응 | 유지할 책임·확인할 연결 |
| --- | --- | --- |
| `main.py`, `diablo2/app.py` | `src/main.rs`, `src/app.rs` | 기본 GUI, `--cli`, Summoner/마을 실행, 인자·기본값·배타 조건·종료 코드 |
| `diablo2/common/config.py` | `src/common/config.rs` | 설정 모델/로더/캐릭터·프로필 선택을 한 소유자에 유지 |
| `common/frame_geometry.py`, `capture.py`, `controller.py`, `movement.py` | 같은 이름의 `src/common/*.rs` | 분석 좌표·실제 창·캡처·입력 변환과 held 입력 소유권 |
| `common/survival.py`, `casting.py`, `buffing.py`, `field_control.py` 등 | 대응 공용 모듈 | 입력 없는 정책·상태와 실제 생산자/실행기의 경계 유지 |
| 공용 `*_vision.py`, `scene_motion.py`, `detectors.py` 등 | 대응 모듈 + 필요한 `common::vision_ops` | 기존 탐지 규칙은 소비자에, 여러 소비자가 쓰는 OpenCV 대체 연산은 공용 구현에 둠 |
| `diablo2/actions/` | `src/actions/` | 방 생성/종료, 조합, 일반 줍기, 녹화 세션 |
| `runs/base.py`, `runs/__init__.py` | `src/runs/base.rs`, `src/runs.rs` | 공용 run 정의와 현재 공개 재수출/프로필 해석; Summoner 연결 누락 금지 |
| `runs/summoner/` 및 `routes/` | `src/runs/summoner/` 및 `routes/` | 현재 실행 흐름·판독·탐색·전투·귀환/협곡 마감·경로 메타데이터 |
| `common/realtime.py`, `field_runtime.py`, `runs/summoner/search_runtime.py` | 대응 모듈 | 독립 계약/API와 테스트를 보존하되 현재 GUI main에 새로 결합하지 않음 |
| `ui/gui.py` | `src/ui/host.rs`, `view.rs`, `model.rs`, `runtime.rs`, `native.rs` | 그리기/표시 상태와 실제 세션·OS 효과를 분리하며 배치 의도, 값, 선택·시작·중지·오류·로그 흐름 유지 |
| `core/bot.py` | `src/core.rs`, `src/core/native.rs` | 구형 CLI 캡처/템플릿/overlay preview. 현재 Summoner 엔진과 구분 |
| `diablo2/tools/` | `src/tools.rs` + `src/tools/`, 같은 EXE의 서브커맨드 | 기존 도구별 인자·입출력·재생 의미. [실제 명령 대응](../../features/rust-runtime.md#developer-command-mapping)을 유지 |
| `config/`, `assets/`, `docs/` | 기존 경로 그대로 | JSON 키·프로필·경로 참조·자산 manifest와 문서 소유권 공유 |
| `tests/test_*.py`, 재생 자산 | Rust 단위/통합 비교 + 기존 Python 회귀 | 같은 사례·가짜 시계·관찰·입력 결과를 비교; 39개 파일 복사 자체를 완료 기준으로 삼지 않음 |

현재 활성 흐름은 `main.py → app.main → GUI → SummonerGuiSession → main_run.run → run_once → continuous_navigation / encounter_execution / canyon_finish`이며, 방 진입·종료에는 `actions/run_lifecycle.py` 등이 연결된다. `--summoner-run`은 `app.main → main_run.run`, 구형 `--cli`는 `app.main → core.bot.DiabloBot → capture/detectors/controller`다. [실제 앱 분기](https://github.com/chochita0311/d2bot/blob/cde68bc2eb1285eddcd8a261cdfe411ed4924f76/diablo2/app.py), [GUI 세션](https://github.com/chochita0311/d2bot/blob/cde68bc2eb1285eddcd8a261cdfe411ed4924f76/diablo2/runs/summoner/gui_session.py), [현재 main](https://github.com/chochita0311/d2bot/blob/cde68bc2eb1285eddcd8a261cdfe411ed4924f76/diablo2/runs/summoner/main_run.py)을 함께 비교한다. 위 표는 조사한 책임 대응안이며, 실행 전 Step 0에서 간접 호출·쓰기·오류 경로까지 파일/심볼 단위로 완성해야 한다.

패키지 구조만을 위해 `app/automation/services/models`를 새 계층으로 추가하거나 `common`을 여러 crate로 찢지 않는다. 독립 배포/재사용, 빌드 의존성 격리, 별도 소유권이 실제로 필요해지면 그 근거와 비용을 다음 구조 변경으로 검토한다. PyO3 또는 Python이 Rust를 부르는 상시 혼합 실행은 기본안에 포함하지 않는다.

현재 패키지 안의 [routes/docs/north_go.md](../../features/summoner-north-route-history.md)는 삭제된 실행기의 역사적 참고다. 이 파일에 나온 옛 실행기·자산을 포팅 대상으로 복원하지 않는다. 문서의 현재 경로와 참조는 유지하고, 나중에 Python 소스를 정리할 때 문서의 영구 위치를 먼저 결정해 이동·링크 수정을 함께 검증한다.

### 구현 도구 후보와 보류 경계

- Windows: Microsoft [windows-rs](https://github.com/microsoft/windows-rs)로 현재 사용하는 표준 OS API를 대응시킨다. OS API 사용은 허용된 경계이며 게임 프로세스 내부 접근을 뜻하지 않는다.
- 설정: `serde`/`serde_json` 후보. JSON 객체 삽입 순서가 기본 선택에 영향을 주므로, [기본 map은 정렬되고 `preserve_order`로 순서를 보존한다는 계약](https://docs.rs/serde_json/latest/serde_json/map/index.html)을 반영한다. 최종 모델 저장 구조도 순서를 잃지 않아야 한다.
- 이미지: Rust 이미지 버퍼/코덱 라이브러리와 필요한 직접 연산을 조합한다. `image`/`imageproc`는 후보이며 API 이름만으로 OpenCV 동등성을 가정하지 않는다. 실제 버전·feature·하위 네이티브 의존성을 선택한 뒤 고정한다.
- GUI: [egui/eframe](https://docs.rs/eframe/0.37.0/eframe/)0.37.0을 정확히 고정했다. 한글·배치·가짜 세션과 native 창 검증은 [GUI run](../run/run-20261023-05-rust-desktop-control.md), 실제 녹화·입력·중지와 최종 전환 검토는 [배포 run](../run/run-20261023-09-rust-windows-distribution.md)이 소유한다. 다른 Windows/GPU/DPI 환경 전체를 검증한 것은 아니다.
- 동시성: 우선 표준 스레드·동기화·채널로 현재 worker와 입력 해제 경계를 표현한다. Rust 전환만을 이유로 Tokio, ECS, 새 이벤트 버스 또는 미래 recording runtime을 도입하지 않는다.
- 녹화: MJPEG를 구현·측정한 뒤 사용자가 MPEG-4 압축기도 Rust로 직접 구현하도록 명시했다. AVI/무음/8fps와 native 크기를 유지하는 구현의 비트스트림·크기·화질·속도·자원 검사는 [EVAL-0028](../evaluation/eval-0028-rust-mpeg4-recording.md), 최종 앱 연결·현재 화면 측정은 [배포 평가](../evaluation/eval-0031-rust-windows-distribution.md)가 소유한다. Xvid C/FFmpeg runtime은 채택하지 않는다.

## Non-Goals

- Python 모듈 이동/삭제와 기존 기본 시작 경로 전환을 초기 기반 작업에 섞는 일. 설치·Cargo 초기화·포팅은 현재 사용자 위임 범위의 feature/spec/run에서 단계별로 진행한다. 실제 게임 입력은 기존 복구 전제를 유지한다.
- OpenCV/NumPy 전체의 범용 호환 라이브러리 개발. 현재 프로젝트가 사용하는 연산과 입출력만 대상이다.
- 포팅 중 탐지 임계값 완화, 경로/전투 개선, 입력 대기 단축, 기본값 정리, 모든 GUI 액션의 dry-run/pause 통합.
- 현재 GUI main을 `RealtimeVisionRuntime`/`FieldRuntime`/`ArcaneSearchRuntime`으로 통합하거나 [PRD-0008](../prd/prd-0008-realtime-runtime-and-recording-retention.md)의 저장 분리·retention을 구현하는 일.
- GPU 캡처/API 교체, 새 해상도·언어·게임 UI 자산 지원, 단일 EXE 안에 모든 설정·자산을 내장하는 최적화.
- 기존 실게임 실패 수정·복구·최종 30방/27방 수용을 포팅 검증으로 대체하는 일. 공개 저장소에 개인 설정·계정 화면·로컬 진단을 자동으로 추가하는 일.

## Invariants

| 경계 | 보존할 규칙과 비교 조건 |
| --- | --- |
| 정책 | [화면 캡처·CV·표준 Windows 입력](../../project/policy.md) 경계를 유지한다. 메모리/패킷/주입/후킹/우회 기능은 추가하지 않는다. |
| 설정 읽기·선택 | JSON 단일 파일/디렉터리, UTF-8 BOM 허용, 정렬된 재귀 파일 순서, 객체 재귀 병합·scalar/list 교체, 키 삽입 순서, 첫 캐릭터·첫 run·`farm` 우선순위·선호 프로필 적용을 보존한다. 누락/null/빈 값·bool/int·지원하지 않는 값의 기존 허용/거부를 구분한다. |
| 파일 경로 | `--config`, 템플릿·manifest 참조, 현재 작업 폴더 기준 상대 경로와 출력 위치를 감사한다. `config/`·`assets/` 복제나 manifest 일괄 경로 변경은 하지 않는다. EXE 위치를 새 기본 기준으로 삼으려면 의도한 변경으로 승인한다. |
| 프레임·좌표 | BGR `uint8` 3채널, 실제 target/client/기준 layout을 구분한다. native 기록과 정규화 분석을 유지하고, 현재 layout의 client 비율 허용 1%, 픽셀 중심 매핑·Python 반올림·ROI 끝 배제·축소/확대 보간·음수 모니터 원점·입력 전 기하 변경 거부를 비교한다. 마을1267×753→분석→물리 좌표는 각각 한 번 변환한다. |
| 캡처 실패 | 현재 `window` 실패는 오류, `auto`는 window 실패 후 screen fallback, 명시적 screen/영역/모니터는 해당 경로를 유지한다. 검은 프레임을 임의로 정상 화면으로 보정하지 않는다. 캡처 API 변경으로 가림·색상·실패 의미를 함께 바꾸지 않는다. |
| 시간·관찰 | `FramePacket.timestamp`의 wall-clock 초와 제어용 monotonic 초를 구분한다. 소비자별 sequence/generation·과거/미래/중복 거부, 캐릭터/방/이동 경계를 보존한다. 현재 이동 입력은 `0 <= now - acquired < 0.25`이며 별도 `now < deadline`도 필요하다. 250ms는 처리 성능 측정값이 아니다. |
| 상태·확인 | 관찰 불가를 0/안전/성공으로 바꾸지 않는다. 입력 요청·전송·버프 효과·포션 소비·처치·획득 확인은 각각 구분한다. 현재 main의 요청 기반 버프 갱신을 공용 confirmed-effect 타이머로 바꾸지 않는다. 캐릭터별 순서·반복 토큰·간격·FCR/생존/벨트 정책과 취소·불완료 결과를 유지한다. |
| 입력·실패 | 한 held 입력 소유자, 독립 해제 시한, 생존 우선 처리, 포커스/stop/pause 경계, 실패 시 key/button 해제, 실제 포인터 역변환·전송 확인과 무단 재시도 금지를 비교한다. 새 입력을 거부해도 정리용 release가 막히면 안 된다. Rust 오류는 기존 오류 범주·외부 메시지/결과로 대응시킨다. |
| 서로 다른 실행면 | GUI Summoner는 현재 live·자동 실행이며 공란=1회/1–5회다. Room Lifecycle 공란은 중지할 때까지 반복이다. CLI의 `dry_run`, F8 pause/F9 stop·preview Esc와 GUI Stop Action/F10, Summoner 필드 F8 stop의 차이를 보존한다. 개발 비교 harness는 가짜 backend를 사용하며 이를 제품 기본값 변경과 혼동하지 않는다. |
| 동시성·시작 | 현재 활성 main의 프레임 선택과 같은 프레임 병렬 관찰, deadline·취소·늦은 결과·worker 오류·종료 순서를 보존한다. 별도 realtime 모듈이 활성 GUI 흐름에 이미 결합됐다고 전제하지 않는다. 공개 run 재수출, 프로필 해석, GUI worker/event 연결, CLI 도구 등록을 점검한다. |
| GUI | DPI 설정은 창 생성 전 적용, helper 창을 캡처 대상으로 잘못 고르지 않기, 창 크기/배치 의도·한글·선택값·버튼 활성/배타 실행·상태·오류·로그 상한을 유지한다. 로그 패널이 늘어나고 우측 제어판은 자연 폭을 유지하는 규칙도 검증한다. |
| 기록·출력 | 기준 Python의 GUI native XVID/AVI, `SessionRecorder`의8fps 설정과 별도 캡처 샘플링, CLI overlay 포함 프레임, snapshot/런 PNG·JSON·JSONL을 구분한다. Rust 압축은 아래 승인한 MPEG-4 직접 구현 차이를 적용한다. 경로·이름 규칙·필드·순서·기본값·성공/중단/실패·증거 출처 의미를 비교한다. UUID/절대 시각처럼 달라질 값은 비교용으로만 치환하고 실제 출력에서 제거하지 않는다. |
| 자원·로컬 데이터 | 캡처/DC/bitmap/키·마우스/encoder/worker를 실패·중지에도 정리한다. 기존 writer 종료와 부분 파일·삭제 실패 의미는 따로 감사한다. 필요한 자산은 기존 owner에, 영구 비교 자료는 명시한 fixture owner에 둔다. 임시 진단은 작업 소유 범위에 두고 활성 writer 종료·필요 자료 승격 후 제거한다. |

## Planned Work

각 Step은 의존성과 위험에 따른 검증 묶음이다. 구현 작업 목록이나 실행 가능한 spec을 확정한 것이 아니다. Step 0의 승인 경계를 거친 뒤, 실제 작업을 한두 계약씩 끝낼 수 있는 feature로 나누고 각 승인 범위만 실행한다.

### Step 0 — 목표·전체 대응표·비교 기준 고정

Goal: 구조·호환 범위를 검토하고 Python 비교 기준과 누락 없는 경로 목록을 확정한다.

Why this grouping: 언어 변경과 비전 재구현은 기존 패키지 정리보다 범위가 커서, 기준과 승인 없이 구현하면 차이의 원인을 추적할 수 없다.

Guardrails: 기존 트랙의 승인·성공 수치를 전용하지 않는다. 포팅 상위 범위는 별도 PRD 초안/사람 검토로 확정하고, 승인된 PRD에서만 foundation/product feature를 제안한다. 이 계획 자체를 spec·실행 승인으로 쓰지 않는다.

Targets: 본 계획, 기존 PRD-0002와 좌표 트랙의 관계, `main.py`/`diablo2/` 전체 공개·활성·보조 경로, `tests/`, config/assets manifest, [.venv 기준](https://github.com/chochita0311/d2bot/blob/cde68bc2eb1285eddcd8a261cdfe411ed4924f76/docs/project/developer-guide.md#interpreter-diagnostics).

Validation: SHA·작업 트리·실제 의존성 버전/OpenCV 빌드 옵션·설정/자산 경로와 SHA-256·필수 frame/시계/난수 사례를 고정한다. 유지할 각 옛 파일/심볼→새 모듈→소비자/실행 분기→읽기/쓰기/출력/실패→테스트를 매핑한다. 기존 전체 `unittest`를 입력 없이 실행해 기준 실패와 미검증을 기록한다. 필요한 shared fixture만 영구 owner에 두고 개인 원본을 자동 공개하지 않는다.

Exit gate: 사람의 구조·호환 범위 검토가 끝나고, 실행하려는 첫 feature의 의존성·계약·판정 기준이 승인된다. 미확정 코덱/GUI 사항은 영향받는 feature를 승인하기 전까지 해소해야 한다.

### Step 1 — Windows Rust 환경과 최소 패키지

Goal: 검증 가능한 native Windows 개발 환경과 입력 없는 최소 빌드 구조를 마련한다.

Why this grouping: 컴파일러/링커 문제를 영상 알고리즘이나 GUI 문제와 분리한다.

Guardrails: Python 시작 경로와 공용 config/assets를 이동하지 않는다. 설치와 생성은 후속 승인 범위에서 수행한다. 외부 계정/서비스 또는 WSL을 새 전제로 두지 않는다.

Targets: MSVC C++ Build Tools·Windows SDK·rustup의 host 대응 MSVC toolchain, 루트 Cargo 파일/`src/main.rs`/`src/lib.rs`, `.gitignore`의 `target/`, README 설치 구분.

Validation: 현재 세션에서는 `rustup/rustc/cargo/cl`을 PATH에서 찾지 못했고 기본 `.cargo/bin`의 rustup/cargo도 없었다. 다른 위치 설치나 MSVC/SDK 전체 설치 상태는 미확인이다. 먼저 이를 확인하고 [공식 MSVC 준비](https://rust-lang.github.io/rustup/installation/windows-msvc.html)와 [Rust 설치 안내](https://rust-lang.org/tools/install/)를 적용한다. 새 PowerShell에서 `rustup show`, `rustc -Vv`, `cargo -V`, 최소 링크 빌드·테스트를 확인한 버전을 `rust-toolchain.toml`/`Cargo.lock`에 고정한다.

Exit gate: Python 환경을 유지한 채 최소 Rust 빌드·테스트가 통과하고 실행 중 게임 입력·불필요한 캡처가 없으며, 실제 target/버전/빌드 도구 경로를 재현할 수 있다.

### Step 2 — 고위험 세 부분의 선행 검증

Goal: 템플릿 매칭, 배경 이동 추정, 기존 녹화의 대체 가능성을 전체 포팅 전에 판정한다.

Why this grouping: 나머지 앱을 모두 옮긴 뒤 순수 Rust 호환성 한계를 발견하면 되돌림 비용이 커진다.

Guardrails: 가짜 입력과 저장 frame만 사용한다. 임계값 완화·알고리즘 교체·다른 녹화 형식으로 통과시키지 않는다. Python/OpenCV를 최종 실행 의존성으로 남기는 fallback은 확정 목표에 어긋난다.

Targets: `common/detectors.py`, `scene_motion.py`, `capture.SessionRecorder`, `actions/recording.py`; 후보 Rust 연산/코덱과 비교 fixture.

Validation: `TM_CCOEFF_NORMED`의 채널별 평균 제거·점수·최고점 위치·동점·상수/빈/부적합 입력·threshold 경계를 비교한다. [OpenCV 수식](https://docs.opencv.org/4.13.0/df/dfb/group__imgproc__object.html)과 [imageproc의 normalized correlation](https://docs.rs/imageproc/latest/imageproc/template_matching/enum.MatchTemplateMethod.html)은 같은 연산이 아니므로 그대로 치환하지 않는다. ORB 특징점/descriptor·Hamming 후보 순서·RANSAC 표본/종료·inlier/오차와 이동량·실패 사유를 연속 프레임에서 비교한다. XVID/AVI는 encoder 사용 가능성, 크기·색상·FPS·재생·중도 실패/마감을 검증하며 인코더 내부 언어와 배포 의존성도 확인한다.

Exit gate: 세 영역 모두 승인한 수치/행동 계약과 의존성 범위를 만족하거나, 불가능/불확실한 부분과 대안을 사람 검토로 돌려보낸다. 하나라도 미해결이면 해당 종속 포팅과 “완전 제거 가능” 결론을 보류한다.

### Step 3 — 설정·좌표·입력 없는 정책/상태

Goal: OS 입력 없이 실행할 수 있는 공용 계약을 Rust로 옮긴다.

Why this grouping: 결정적인 입출력·가짜 시계로 수치·타입·상태 의미를 분리 검증할 수 있다.

Guardrails: `common`/`runs/base` 책임을 유지한다. `Option`, 정수 타입, map 자료구조 도입을 이유로 기존 null/기본값/순서/오류 조건을 더 엄격하거나 느슨하게 만들지 않는다. 화면 normalize 연산은 Step 4와 연결하되 좌표 계약부터 확인한다.

Targets: config/캐릭터 선택, frame geometry, casting, survival, buffing, field-control/관찰 출처, run 정의·cycle 계약. 대응 Rust 모듈과 공유 비교 데이터.

Validation: JSON 병합·선택과 path/BOM 사례, 반올림 ±0.5·음수 원점·왕복 좌표·기하 변경, class/FCR 미설정·잘못된 bool/int, 생존/벨트/버프의 unknown·stale·future·세대 변경을 비교한다. `test_frame_geometry`, `test_casting_rules`, `test_survival_state`, `test_buffing`, `test_buff_renewal`, `test_field_control`, `test_realtime`, `test_summoner_cycle`의 해당 사례를 Rust 비교에 연결한다.

Exit gate: 같은 설정·관찰·시계·이벤트에 대해 선택값·상태 전이·보류/요청·오류가 일치하고, 공개 모듈/재수출과 소비자 연결이 테스트된다.

### Step 4 — 필요한 비전 연산과 판독기

Goal: Step 2에서 입증한 핵심 연산을 포함해 현재 판독 흐름을 OpenCV/NumPy 없는 Rust 구현으로 이전한다.

Why this grouping: 공용 연산의 차이와 도메인 판단의 차이를 별도로 추적해야 잘못된 임계값 보정을 막을 수 있다.

Guardrails: 기존 자산/ROI/전처리 순서/판정 조건을 유지한다. 연산별 허용 오차를 결과를 보고 넓히지 않는다. 범용 비전 프레임워크나 외부 대형 OCR을 새로 추가하지 않는다.

Targets: 공용 `vision_ops`, 색/마스크/배열·거리/필터/이진화/형태학/연결 영역, 영상 파일 I/O·overlay, HUD 숫자/벨트·buff·automap·encounter·label, Summoner 지형/카메라/중앙/끝/일지/목표/생존 판독기.

Validation: BGR↔Gray/HSV/Lab 범위·정밀도·반올림·overflow·ROI·보간·필터 경계와 component 정렬을 비교한다. 픽셀/마스크/score/box/특징/이동량뿐 아니라 탐지·경로·보류/실패 사유까지 비교한다. `test_survival_vision`, `test_camera_ground`, `test_automap`, `test_encounters`, `test_label_occlusion`, `test_arcane_*`, `test_summoner_life_vision` 등 소비자별 기존 양성/음성·가림·불명 사례를 사용한다. 이미지 decode/encode와 한글 경로·깨진 파일·누락 template도 포함한다.

Exit gate: 이전한 각 판독기의 수치 계약과 최종 판단이 고정 자료에서 일치한다. 미감사 연산·입력 조건은 명시하고 그 경로의 동등성은 `Provisional`로 유지한다.

### Step 5 — Windows 캡처·입력·독립 해제

Goal: 기존 OS 연동과 입력 보호/정리를 Rust에 연결한다.

Why this grouping: 실제 side effect가 시작되는 경계이므로 순수 계산과 별도의 승인·검증이 필요하다.

Guardrails: 먼저 fake backend→입력 없는 캡처→승인된 짧은 감독 입력 순서다. 실제 게임에 Python/Rust가 동시에 입력하지 않는다. 새 캡처 엔진·자동 재시도·현재 창의 과거 상태 재사용은 포함하지 않는다.

Targets: window 탐색/DPI/client bounds, PrintWindow/BitBlt·screen 대응, keyboard/mouse/SendInput·포인터 readback, stop/hotkey·held-key watchdog·자원 해제. `unsafe` OS 호출은 이 실제 연동 경계 안에 한정한다.

Validation: 모드별 fallback/실패, 창 이동·축소·가림·음수 모니터·geometry 변경, 같은 frame normalize를 비교한다. `test_field_pointer`, `test_field_control`, `test_field_input_probe`, `test_frame_geometry`의 실패/stop/focus/release 사례와 지연 worker·panic/오류 정리를 확인한다. 포인터 확인의 현재 허용 오차/기한/재시도 금지 및 키·클릭 hold 시간을 실제 코드 기준으로 고정한다. 현 세션의 게임 복구 전제는 새로 확인해야 하며 예전 관찰이 입력 허가를 대신하지 않는다.

Exit gate: capture/입력 경로의 정리·실패 감사와 승인된 Windows 검증이 통과한다. 느린 비전/저장/GUI 작업 때문에 입력 해제가 기다리지 않음을 확인한다.

### Step 6 — 현재 run·action 실행 흐름 연결

Goal: 현재 활성 Summoner와 다른 action, 구형/보조 runtime을 각 계약대로 연결한다.

Why this grouping: 공용 단위 검사가 같아도 호출 순서·등록·보고서 쓰기·부분 실패가 달라질 수 있다.

Guardrails: 활성 main을 기준으로 옮기며 기존 realtime API와 임의로 통합하지 않는다. navigation·encounter·귀환·방 마감은 비교 가능한 묶음으로 나눠 검증한다. 미완료 실제 처치/획득/전체 완주를 구현 완료로 기록하지 않는다.

Targets: app 실행 분기, run 재수출/resolve, `main_run`/`continuous_navigation`/encounter/canyon/lifecycle, gem/loot, 별도 field/search runtime과 재생 API, 결과/증거 writer.

Validation: 같은 관찰 순서·가짜 시계·입력 backend로 방 진입→버프 요청→이동/전투/줍기→귀환/협곡→Act1/종료의 상태·입력 순서·deadline·보고서/이벤트를 비교한다. 실패/자원 부족/stop/focus loss/부분 기록/이후 반복 거부도 감사한다. `test_summoner_main_run`, `test_continuous_navigation`, `test_continuous_steering`, `test_summoner_encounter_execution`, `test_summoner_canyon_finish`, `test_north_room_cycles`, `test_town_loop`, `test_emergency_recovery`, `test_conditional_loot`, 별도 runtime 테스트를 연결한다.

Exit gate: 활성·보조 각 약속된 흐름의 wiring/startup/replay와 side effect·실패 비교가 통과한다. 직접 게임 검증이 필요한 부분은 사람 복구 확인과 해당 승인 뒤 한정 실행하고, 재생 통과와 구분해 기록한다.

### Step 7 — GUI·CLI 도구·녹화의 외부 계약

Goal: Rust에서 기존 제어판과 모든 유지 대상 실행 인터페이스를 제공한다.

Why this grouping: operator가 보는 선택/시작/중단/결과와 녹화/preview 계약을 실제로 확인해야 전환을 검토할 수 있다.

Guardrails: GUI framework 교체를 화면 재설계로 확대하지 않는다. CLI 플래그/종료 코드·기능별 공란 반복 의미·현재 live/dry/pause 차이를 유지한다. XVID 불가를 PNG/MP4 저장으로 바꾸어 완료 처리하지 않는다.

Targets: `ui/gui`, app parser, `core/bot`, `tools`와 필요한 bin 등록, recording/snapshot/overlay, 세션 worker와 이벤트·로그 표시, [GUI 유지 지침](../../project/gui-maintenance.md).

Validation: 입력 없는 GUI 시작에서 창 크기·DPI·한글·로그 확장·캡처 후보·캐릭터/난이도·액션 배타/버튼 상태를 스크린샷과 직접 관찰로 비교한다. `test_gui_summoner`, `test_summoner_gui_session`, `test_summoner_session`의 선택/반복/취소/실패 의미와 CLI help·잘못된 옵션·출력/exit code를 확인한다. Step 2의 코덱 결과를 실제 native GUI 녹화/CLI overlay/마감·재생에 적용하고, 각 도구는 기존 `python -m ...`과 새 실행 명령의 대응·출력 의미를 검증한다.

Exit gate: 모든 유지 대상 사용자 동작과 도구가 등록되고 입력 없는 시작/중단·기록 재생이 통과한다. framework 때문에 생기는 외관 차이·누락 기능은 사람 검토 없이 수용하지 않는다.

### Step 8 — 전체 비교·배포·전환

Goal: Python/OpenCV 없는 Windows 배포를 검증하고 기본 실행 전환 여부를 판단한다.

Why this grouping: 개발 PC에서의 성공과 배포 의존성 제거·전체 호환성은 서로 다른 증거가 필요하다.

Guardrails: 미감사 기능을 조용히 빼거나 빠른 성능을 동등성 증거로 사용하지 않는다. 전환 전까지 Python 기본 실행/회귀 수단을 유지한다. 전환과 Python 코드/환경 삭제는 구분하고, 후자는 별도 명시적 범위에서 한다.

Targets: 전체 대응표·회귀/비교 보고, release 빌드·실행 의존성·config/assets 배포, README/개발·구조 문서와 본 계획의 상태. 실제 실행 증거는 해당 run/evaluation이 소유한다.

Validation: 승인된 feature의 확정 상태에서 Python 전체 회귀와 Rust 전체 테스트·release 빌드를 수행한다. Python/OpenCV가 없는 Windows 검증 환경에서 GUI/CLI·자산/설정·preview/녹화·중단을 확인하고 `cargo tree`의 feature/빌드 의존성과 최종 DLL·코덱을 감사한다. 같은 PC/해상도/자료에서 p50/p95 단계 지연·관찰→입력 지연·stale_hold 비율·중지 지연·CPU/메모리·시작 시간·배포 전체 크기를 비교한다. 수용할 성능 한도는 측정/전환 전에 승인한다.

Exit gate: 약속된 전체 경로의 엄격 감사·수치/판단/입력/출력/실패 비교·의존성 제거 증거가 갖춰지고 사람이 전환을 수용한다. 부족하면 Python으로 돌아가고 정확한 미완료 경계를 남긴다. 기존 게임 목표 수용 상태는 별도로 유지한다.

## Validation Gates

1. **계획과 기준**: Step 0의 범위/승인, 불변 SHA, config/assets/fixture의 경로·해시, 실제 Python/CV 빌드 환경을 먼저 고정한다. 이후 각 feature는 `docs/agents/` 절차로 승인하고 실행한다.
2. **정적·빌드**: Rust 코드가 생긴 뒤 `cargo fmt --check`, `cargo check --all-targets --locked`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --all-targets --locked`를 해당 범위에 적용한다. 배포 gate는 `cargo build --release --locked`. optional feature/추가 bin을 채택하면 그 실제 조합도 검사한다.
3. **Python 기준 회귀**: 프로젝트 인터프리터로 `.\.venv\Scripts\python.exe -m unittest discover -s tests`를 기준 고정과 각 단계 종료 시 가능하면 확정 코드 상태에서 실행한다. Python을 수정하면 개발 지침에 따라 Black/compile도 수행한다. 최초 계획 작성에서는 미수행이었고, 전체 실행 전환 뒤 실제 결과는 [현재 run](../run/run-20261022-01-rust-native-build.md)이 소유한다.
4. **비교 단계**: 픽셀/수치→탐지·상태/실패→가짜 입력 trace→파일/이벤트→입력 없는 Windows/GUI→승인된 감독 입력→배포 순서다. fake clock/고정 난수·동일 설정과 자료를 사용한다. RANSAC 재현성 한계는 별도 기록하고 미확인 결과를 동등으로 처리하지 않는다.
5. **판정 허용 오차**: 연산별 절대/상대 오차·위치 오차를 비교 전에 정한다. 수치 오차가 있어도 최종 분기·실패/보류·입력 계획의 미승인 차이는 0건이어야 한다. 예를 들어 탐지기의 `score < threshold`일 때 거부/동일값은 수용을 `<`, `=`, `>`로 검사하고, 화면 나이는 음수·249ms·250ms·초과와 deadline 직전/동일값을 구분한다. Python ties-to-even 반올림도 ±0.5 경계로 확인한다.
6. **엄격 동등성 감사**: 모든 유지 경로에 old→new 파일/심볼·entry→service/runtime→읽기/선택→쓰기/output 매핑을 기록한다. 이 프로젝트에는 DB query/transaction이 없으므로 해당 비교는 config/asset/화면/대상 선택 조건과 파일·상태 갱신의 부분 실패에 적용한다. 필드·기본값·정렬/중복·시각·이벤트/로그·입력/핫키·캐시/관찰 무효화·재시도/정리/부분 완료·권한/포커스 경계를 감사한다.
7. **주장과 기록**: 미감사 경로가 하나라도 있으면 전체 parity는 `Unknown/Provisional`이다. 수행 이력/첫 blocker/runtime smoke는 refactor log 또는 이미 해당 범위를 소유한 run/evaluation에 기록하고 중복하지 않는다. 실제 parity/merge 판단을 요청받는 시점에 스킬의 merge-check를 작성한다. 전체 테스트 통과만으로 실제 게임 성공·축소 화면 정확도·완주를 주장하지 않는다.
8. **범위와 되돌림**: 단계 종료마다 해당 변경만 diff로 검토한다. Python 기본 경로와 이전 확정 코드를 되돌림 수단으로 유지하고, 검증이 끝나지 않은 다음 단계 코드·진단·무관한 포맷 변경을 섞지 않는다. 기준을 바꾸면 원인·차이·재실행할 비교를 기록한다. task 임시 출력은 writer 종료 후 필요한 결과를 승격하고 정리한다.

## Intentional Deltas

| 변경 | 상태·수용 조건 |
| --- | --- |
| Python/OpenCV/NumPy 실행 경로를 Rust 앱·필요 연산으로 대체 | Python·OpenCV 최종 배포 제거는 사용자 확정 목표. Python 비교 환경은 개발 중 유지. 대체 코드의 동작 동등성은 아직 미입증 |
| Cargo 파일·`src/` 추가, Rust 타입·모듈/오류·자원 관리 | 구조 권고안. Python 문법·객체·traceback 자체의 동일성을 요구하지 않되 외부 오류 범주/메시지·상태·정리 순서는 비교 |
| Tkinter를 Rust GUI로 교체, Rust 실행 명령 제공 | 구현 기술/명령은 달라진다. 기존 버튼/값/배치 의도·조작 의미·CLI 옵션/결과 대응은 유지. 픽셀 수준 허용 차이는 GUI 승인 시 확정 |
| 잘못된 선택적 버프 대기시간 보정 | `2026-10-22` 사용자 승인. Python float 변환이 허용한 NaN/무한대·overflow와 음수/변환 불가 override를 제거하고 기존 소비자의 기본값으로 계속 실행하며 경고한다. 정상 값/다른 필수 정책의 계약 변경으로 확대하지 않음. [SPEC-0016](../spec/spec-0016-rust-config-contract.md)이 경계 소유 |
| JPEG 참고 화면의 디코더 차이 | `2026-10-23` 사용자 승인. 순수 Rust JPEG의 측정된 채널≤3/255·세 floating center·한 rounded center1px 차이만 허용하며 capture/PNG·탐지 여부/선택 템플릿/임계값은 유지한다. [SPEC-0018](../spec/spec-0018-rust-vision-operations.md)의 고정 참고 비교 한도가 경계를 소유 |
| 직접 Rust MPEG-4 압축 | `2026-10-23` 사용자 확정. Xvid C 라이브러리와 동일한 구현/압축 바이트는 요구하지 않되 AVI/무음/8fps/native 크기와 호환 재생·품질·효율·마감을 검증한다. MJPEG는 용량 비교 실험으로 남기고 최종 기본 코덱으로 채택하지 않음. [RUN07](../run/run-20261023-07-rust-mpeg4-recording.md)이 현재 증거와 한계를 소유 |
| EXE 기준 경로 탐색, 프레임/스레드·대기 정책 개선 | 최초 포팅에서는 **미승인 대안**이었고 기존 동작 보존에 포함하지 않았다. 이후2026-10-24 사용자가 승인한 같은 목적의 계산/소유 구조 개선은 [후속 효율 트랙](refactor-0003-rust-efficiency.md)이 소유한다. EXE 기준 경로 탐색과 입력/대기 의미 변경까지 승인한 것은 아니다 |
| 성능·배포 크기 개선 | 측정한 병목의 순수 계산 최적화는 최종 spec 안에서 같은 픽셀·합산/반올림·판정·실패 계약으로 재검증한다. 정확한 축/반사 인덱스, 동일 불변 Arc의 패널 bool, 관찰 내 불변 crop의 누적합과 최대8개/precision 불변 FFT 알고리즘을 재사용하며 새 캡처는 반드시 다시 판독한다. FEAT-0028 당시 FFT/area/ORB 내부 최대4개 scoped worker는 반환 전에 모두 join했다. 이후 최대8개 공유 풀과 자료 수명/배포 자산 변경은 FEAT-0029가 소유한다. 화면 시한·기존 관찰 pool/대기·입력 정책은 바꾸지 않는다. 릴리스 thin LTO/단일 codegen unit도 수치/실패 회귀 후 채택하며 속도 향상·단일 파일·고정 크기를 보장하지 않는다 |

## Risks / Open Questions

| 항목 | 현 상태·권고·결정 시점 |
| --- | --- |
| 구조 선택 | 사용자의 표준적 구조·전체 실행 위임에 따라 동일 저장소·루트 Cargo·단일 패키지·역할별 Rust 모듈로 시작한다. 새 `rust/` 하위 프로젝트나 다중 workspace는 현재 필요 근거가 없다. 세부 경계는 각 feature/spec에서 확정 |
| 순수 Rust 범위 | Python/OpenCV 제거와 녹화용 MPEG-4 압축기의 직접 Rust 구현은 사용자 확정. Windows 캡처·입력·GUI 지원은 표준 OS API를 사용한다. OpenCV 제거와 모든 외부 네이티브 코드 제거를 같은 주장으로 쓰지 않음 |
| ORB/RANSAC·수치 차이 | 특징점 선택/동점·난수·float32/64·색변환·보간 때문에 행동이 달라질 수 있다. Step 2/4에서 비교하고 미해결이면 종속 범위 보류 |
| XVID/AVI | 직접 Rust MPEG-4가 독립 디코딩·정지/이동/장면 전환·크기/품질/시간·오류/마감의 지정 비교를 통과했다. 실제 고지/출처와 전체 profile 미인증 한계는 [RUN07](../run/run-20261023-07-rust-mpeg4-recording.md)이 소유한다. Xvid GPL 라이브러리를 runtime에 연결하지 않으며 새 라이브 화면 검사는 최종 run 소유 |
| GUI·최소 지원 환경 | egui/eframe0.37.0의 실제 제어판·한글·배치·지정 DPI 검사는 [EVAL-0029](../evaluation/eval-0029-rust-desktop-control.md)를 통과했다. Windows/OpenGL 드라이버가 필요하며 미검증 OS/GPU/DPI를 지원 인증했다고 확대하지 않는다 |
| 실행 위치·배포 | EXE/config/assets/고지를 함께 두는 폴더 배포와 기존 CWD 상대 경로를 채택했다. root Cargo package와 정적 CRT 패키징 script를 사용한다. 단일 파일 내장·임의 CWD 자동 탐색은 도입하지 않았다 |
| 미완료 기능과 결함 | 현재 main과 독립 runtime은 같은 통합 상태가 아니다. Python의 미완료 동작을 포팅 성공으로 바꾸지 않는다. RecordingSession의 명시적 capture 정리 누락처럼 발견한 자원/오류 경계는 비교해 두고, 결함 수정이 필요하면 범위·delta를 별도로 승인 |
| 계속되는 Python 개발 | 포팅 중 Python 수정이 들어오면 기준 SHA·설정/자산 해시와 변경 대응표를 갱신하고 영향받은 비교를 다시 수행한다. 동기화되지 않은 두 구현을 모두 최신이라고 표시하지 않음 |
| 실게임 재개 | [2026-10-22 기록](../run/run-20261009-07-summoner-live-integration.md#session-close-20261022)의 복구 대기는2026-10-24 사용자 복구/Act1 저장 종료 확인으로 해소됐다. 새 장면·입력·종료 상태는 [최종 포팅 run](../run/run-20261023-09-rust-windows-distribution.md)이 소유한다. 동일 확인을 반복하지 않으며 새 화면·포커스/HUD·메인 무기/ALT·시한 조건과 원본 수용 실적은 유지 |
| 보관·성능 | 비교 기준에 필요한 소수 자료만 명시한 owner로 유지하고 raw 진단을 무기한 보관하지 않는다. 오래된 화면의 행동 거부와 입력 해제 시한을 유지한 상태에서 성능을 측정하며 개선 폭/수용 한도는 미확정 |

## Exit Goal

**최초 계획 작성의 완료 조건**은 기존 구조를 유지할 이유, 현재→Rust 책임 대응, 고정한 소스 기준과 아직 미고정인 실행 환경, 순수 Rust 전환의 위험·검증 단계·의도한 차이·사람 검토 지점을 문서로 제시하는 것이었다. 이후 전체 실행 위임을 받아 PRD-0009와 feature들로 진행한다. 각 단계의 설치·구현·동등성 결과는 해당 run/evaluation으로 확인한다.

**후속 포팅 트랙의 완료 조건**은 승인한 모든 현재 기능/보조 인터페이스에 대해 엄격 비교 증거가 있고, Python/OpenCV 없는 Windows 배포에서 필요한 설정·자산·GUI/CLI·녹화·중단이 동작하며, 사람이 기본 실행 전환을 수용하는 것이다. 필요한 영구 자료와 문서를 갱신하고 작업 임시 산출물을 정리한다. 그 이후의 알고리즘 개선·새 runtime 통합·기존 Python 제거는 별도 범위로 다룬다.

## Handoff to Next Track

사용자는2026-10-24 실행 가능성 뒤의 ORB/녹화 속도·메모리 잔류를 원인부터 개선하고 같은 목적 안에서 자료/설계·최소 공통 assets를 효율화하도록 지시했다. [REFACTOR-0003](refactor-0003-rust-efficiency.md)과 FEAT-0029가 이 추가 범위를 소유한다. 이전292/native4/Python765와 실행가능 EXE는 변경 전 증거로 유지하며, 모든 측면의 효율 수용·사람 전환·전체 게임 성공으로 확대하지 않는다.
