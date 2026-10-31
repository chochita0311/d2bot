# RUN-20261022-01: Windows Rust 기반 빌드

## Metadata
- ID: `run-20261022-01`; Status: `passed`; Attempt: `2`.
- Feature: [FEAT-0016](../feature/feat-0016-rust-native-build.md); Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Active Spec: [SPEC-0015](../spec/spec-0015-rust-native-build.md).
- Surface: `infra`; Execution Profile: `infra-devtool`; Required Evaluators: Contract, Functional; Created/Updated: `2026-10-22`.

## Goal
공식 도구·고정 Cargo 기반과 입력 없는 EXE smoke를 검증한다. 전체 포팅 완료와 구분한다.

## Selected Loop
- Feature type: foundation; lane 없음; Orchestrator가 범위/추적/평가자를 고정했다.
- Current phase: Attempt2의 설치·실제 link/test/EXE smoke 통과. 다음 FEAT-0017의 설정 계약으로 이동한다.
- 한 feature씩 진행하며 독립한 read-only evidence와 baseline 검사만 병행한다.

## Contract Surfaces
MSVC target/고정 Rust 버전, Cargo layout/lock, 초기 진단 명령/exit와 기존 Python/config/assets 불변.

## Invocation Context
사용자의 전체 실행 위임, PRD-0009/REFACTOR-0002, project 정책/개발 지침, runner/실행 프로필/거버넌스를 적용한다. full app과 실제 game 입력은 이 실행 대상이 아니다.

## Current Artifacts
- Active spec: SPEC-0015.
- Evaluation: [EVAL-0015](../evaluation/eval-0015-rust-native-build.md). 이번 원인은 설치/링크 환경이며 구현 fix log는 해당 없음.

## Evaluation Coverage
- Rust: 1.99.1/host `x86_64-pc-windows-msvc`, rustup1.29.2, rustfmt/clippy 설치·고정. fmt/check/clippy는 통과했고 Attempt2에서4개 unit test·release link·EXE help/version/diagnose·invalid argument exit2가 통과했다. coverage는 이 foundation 경계에서 complete이며 전체 port parity가 아니다.
- Python 최초 기준: 765 tests/112.944초 중 6 errors, 나머지759 통과. 모두 sandbox Temp의 `report.json.pending`→`report.json` replace WinError5였다. 일반 권한 환경에서 해당 main 모듈42 tests/5.513초가 통과했고, 이어 전체765 tests/105.468초가 통과했다. Rust 결과가 아니다.

## Current Route
- Next role: 다음 설정 foundation의 계약/spec 고정 후 Builder.
- Blocker classification: 현재 없음. Attempt1의 Windows 설치 기술 blocker는 Attempt2에서 해소했다.
- Post-run human review: 첫 foundation 결과와 미검증을 보고하고 전체 포팅은 위임 범위에서 이어간다. 새 동작/코덱 호환성 변경은 따로 확인한다.

## Attempts
- Attempt1: Windows x64/NT10.0.26200.0 확인. rustup/rustc/cargo/cl은 PATH에 없고 기본 cargo bin/VS installer/Windows SDK lib 경로에서 확인되지 않았다. 다른 설치 위치의 존재를 부정하는 증거는 아니다.
- Python baseline 측정과 실제 설정/자산/CV 계약 증거 조사를 수행했다. pip show는 cp949 출력 문제로 부분 확인이므로 metadata를 별도 확인한다.
- metadata 확인: `opencv-python==4.13.0.92`, `numpy==1.21.6`, `mss==7.0.1`, `keyboard==0.13.5`, `pydirectinput==1.0.4`, `black==23.3.0`.
- 공식 Rustup1.29.1 설치 후1.29.2로 자체 갱신, Rust1.99.1과 rustfmt/clippy를 별도 정확 버전으로 설치했다. self-update setup 정리는 일반 권한 환경에서 마쳤다. 원래 PATH의 새 도구 반영 전이라 이번 명령은 `.cargo/bin`의 절대 경로를 사용한다.
- Cargo package/lock/toolchain과 `src/{lib,main,app}.rs`를 작성했다. 인자 없는 help, version, 입력 없는 진단, 명령/I/O 오류의 nonzero 결과를 정의했고 현재 main의 실제 입력을 호출하지 않는다. 4개 단위 검사는 작성됐으나 링크 전이라 실행 결과는 없다.
- Attempt1의 `cargo build --release --locked`: `link.exe not found`로 실패. Build Tools는 공식 installer hash 검증 후 관리자 bootstrap 실행에서 종료됐고 VS installer/SDK가 생성되지 않았다. 사람에게 UAC 재요청/직접 설치 선택을 질문해 이 attempt를 기술적으로 중단했다.
- 기존 진단 경로: `C:\Users\cjsth\AppData\Local\Temp\dd_vs_BuildTools_decompression_log.txt`는 vendor-managed 설치 로그다.1602/0x80070642·관리자 프로세스 시작 실패를 위에 요약해 결과가 이 임시 경로에 의존하지 않는다. 설치 복구 중에는 보존하며, 성공한 설치 뒤 vendor cleanup 범위에서 처리한다. 별도 설치 cache의 소유권이 불분명하므로 임의 삭제하지 않는다.
- Attempt2: 사용자가 “승인 창 다시 요청”을 선택했고 같은 공식 Build Tools17.14.42와 VC x64/x86·Windows11SDK22621 설치가 성공했다. vswhere의 isComplete/isLaunchable=true/isRebootRequired=false와 SDK kernel32.lib를 확인했다.4개 unit tests/0실패, release build, 실제 d2bot.exe의 version/diagnose/help/invalid argument exit2를 확인했다. 진단에서 Windows/x86_64·config/assets/Python 비교 소스를 확인했으며 game/input/GUI는 실행하지 않았다.

## Post-Contract Regression Check
기존 Python 소스/config/assets·기본 시작 명령은 변경하지 않는다. 빌드 이후 실제 diff로 확인한다.

현재 diff에서 기존 Python/main/config/assets/tests/requirements/pyproject 변경은 없고, 새 기반 static·4개test·실제link/smoke가 통과했다. Attempt1의 부분 build 출력119 files/2.9MiB는 정리했고 Attempt2는 표준 Cargo target을 재생성했다. 전체 port의 후속 빌드/검증 중에는 같은 표준 target을 사용한다. 문서 검사 이력은23개/180개 상대 링크 통과이며 다음 문서 갱신 후 영향 범위를 다시 확인한다.

## Human Review Outcome
- 이 foundation의 자동 검증은 완료했다. 전체 실행 위임에 따라 다음 bounded feature를 이어가며 최종 앱 전환의 사람 수용은 남아 있다.

## Continuity Notes
- `2026-10-22`: 계획 전용 요청에서 실행으로 전환. 기존 실게임 복구 대기는 계속 유효하다.
