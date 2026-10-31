# SPEC-0015: Windows Rust 빌드 기준

## Metadata
- ID: `spec-0015`; Status: `approved`; Run ID: `run-20261022-01`; Attempt: `2`.
- Parent Feature: [FEAT-0016](../feature/feat-0016-rust-native-build.md); Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Surface: `infra`; Execution Profile: `infra-devtool`; Surface Lane: none; Required Evaluators: Contract, Functional; Created/Updated: `2026-10-22`.

## Source Set
- 사용자 전체 포팅 실행 위임과 부모 문서의 범위.
- [개발 지침](../../project/developer-guide.md), [정책](../../project/policy.md), [포팅 계획](../refactoring/refactor-0002-rust-port.md).
- Rust 공식 rustup/MSVC/Cargo 문서; 현재 Python 코드와 의존성은 변경하지 않는다.

## Implementation Goal
공식 도구로 x64 Windows MSVC 실행 파일을 빌드하는 재현 가능한 Cargo 기반을 만든다. 이 기반은 전체 포팅의 완료물이 아니다.

## In-Scope Behavior
- 기존 설치/PATH를 먼저 확인하고 공식 Rust stable/MSVC build prerequisites를 설치·확인한다. 설치된 도구·버전은 정확히 기록한다.
- 루트 Cargo 패키지 `d2bot`, library `src/lib.rs`, binary `src/main.rs`, dispatch `src/app.rs`를 둔다. public module은 구현된 것만 공개한다.
- 초기 EXE는 `--help`, `--version`, `--diagnose`만 제공한다. 인자가 없으면 현재 이전 단계와 사용법을 표시하고 종료한다. 기능이 없는 GUI/자동 실행을 성공으로 표시하지 않는다.
- CLI 파싱 오류는 stderr와 nonzero, 진단 성공은 stdout과0; 진단은 OS/아키텍처/버전과 현재 경로·비교용 Python 존재 여부만 보고하고 game/input은 실행하지 않는다.
- `rust-toolchain.toml`은 실제 확인한 정확 버전과 rustfmt/clippy를 고정한다. `Cargo.lock`을 추적하고 `target/`는 Git 제외한다.
- 공용 library는 안전한 Rust를 기본으로 시작한다. OS unsafe 모듈이 생기기 전 임의 unsafe/전역 가변 상태/외부 코드 실행을 넣지 않는다.

## Out-Of-Scope Behavior
GUI/CV/녹화/게임 입력 구현, Python 시작점 교체, config/assets 수정·복제, 새 의존성/폴더 재편, 기존 실패 수정.

## Affected Surfaces
Cargo.toml/lock, rust-toolchain.toml, src/{lib,main,app}.rs, .gitignore, README 개발 준비 부분, 본 feature/spec/run/evaluation.

## State And Interaction Contract
- 모든 커맨드는 입력 없는 bounded 실행이다. malformed CLI 인자는 사용법을 표시하고 실패한다.
- 미구현 기능은 feature 상태와 구분해 설명한다. 무인 game side effect, 자기 업데이트/네트워크 호출은 앱에 없다.

## Data And Contract Assumptions
- 코드 비교 기준 SHA는 `cde68bc2eb1285eddcd8a261cdfe411ed4924f76`.
- Windows x64 native/MSVC가 현재 대상이며 실행 환경이 다르면 명확히 거부/보고한다.
- 빌드 결과 `target/release/d2bot.exe`는 Cargo의 반복 빌드 산출물이다. 최종 사용자 배포의 영구 owner는 FEAT-0028에서 정한다.
- 설치된 Rust/MSVC/SDK는 사용자 요청의 개발 도구로 유지한다. task 다운로드/진단은 `.tmp/rust-port-20261022/`에 한정하고 writer 종료 후 제거한다.

## Contract Surfaces
- Producer: Cargo/rustup와 app dispatcher.
- Consumer: Windows terminal, 하위 Rust feature, 기존 Python 사용자는 기존 명령 유지.
- Generated artifacts: Cargo.lock은 해석된 버전 기준으로 추적; target와 task raw diagnostics는 재생성 가능.
- Source-of-truth owner: Cargo 설정·고정 toolchain과 부모 feature. 실제 검사 결과는 run/evaluation.
- Stale-assumption check: README/AGENTS/계획 인덱스가 Rust 전체 포팅 완료를 암시하지 않게 확인한다.

## Required Evaluators
- Contract: target/명령/상태/lock/안전 Rust 경계, 기존 파일 불변, 문서 추적.
- Functional: fmt/check/clippy/test/release, release EXE help/version/diagnose/오류 exit.
- Design/UX: GUI가 없는 이 경계에서는 해당 없음.

## Acceptance Mapping
FEAT-0016의 재현 빌드→고정된 manifest/toolchain/checks; EXE 실행→CLI smoke; 기존 불변→기준 diff/파일 해시와 config/assets 확인.

## Evaluation Focus
링커/SDK 실제 가용성과 release 링크 성공을 구분한다. toolchain 설치만으로 빌드 완료를 주장하지 않는다. bootstrap EXE를 전체 포팅 완료로 주장하지 않는다.

## Open Blockers
현재 blocker 없음. Attempt1의1602/0x80070642와 missing linker는 사용자 UAC 확인 뒤 Attempt2의 공식 Build Tools17.14.42/SDK22621 설치로 해소됐다.4개 test·release link·EXE smoke를 통과했으며 codec/GUI는 후속 범위다.

## Continuity Notes
- `2026-10-22`: 사용자 실행 위임 범위를 첫 bounded foundation으로 고정했다.
