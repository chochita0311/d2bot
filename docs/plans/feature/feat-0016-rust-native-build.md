# FEAT-0016: 재현 가능한 Windows Rust 빌드

## Metadata
- ID: `feat-0016`; Status: `passed`; Type: `foundation`; Surface: `infra`.
- Execution Profile: `infra-devtool`; Required Evaluators: Contract, Functional.
- Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md); Created/Updated: `2026-10-22`.
- Approval: 사용자의 전체 실행 위임 중 의미 변경 없는 공식 개발 환경·최소 패키지 경계. 코드 구현 전 SPEC-0015로 고정한다.

## Goal
고정한 Rust/MSVC 환경에서 기존 Python과 독립된 Cargo 앱을 빌드·실행·테스트할 수 있다.

## Acceptance Contract
고정 toolchain으로 fmt/check/clippy/test/release 성공; release EXE가 --version/--help/입력 없는 진단에서 정상 종료; Python 소스/config/assets 바이트 불변.

## Scope Boundary
- In: 공식 Rust/MSVC/SDK 준비, 단일 Cargo 패키지, lock/toolchain, 입력 없는 진단 진입점, 빌드 명령과 오류 처리.
- Out: GUI·CV·게임 입력·녹화 구현, Python 경로/기본 실행 변경.

## Contract Surfaces
기존 Python의 해당 공개 동작/입출력과 [계획의 invariants](../refactoring/refactor-0002-rust-port.md#invariants)가 기준이다. Rust의 구현 타입·자원 소유권은 각 spec에서 명시하며, 모호한 값·fallback·운영 부작용을 추측하지 않는다.

## User-Visible Outcome
하위 feature가 의존할 수 있는 검사 가능한 계약이다. 이 foundation만으로 완성 앱을 주장하지 않는다.

## Entry And Exit
- Entry: 승인한 실제 호출/진단/테스트 경계에서 시작.
- Exit: 성공·보류·오류·취소를 구분하고 소유한 worker/handle/input을 정리한다.

## State Expectations
- 기본·정상·누락/불명·오류·취소 상태를 구분한다. 기존의 불명을 성공으로 바꾸지 않는다.
- 실제 게임 side effect는 해당 feature/spec의 명시한 경계 안에서만 검증한다.

## Dependencies
없음. 설치 권한·네트워크·디스크는 실제 확인한다.

## Likely Affected Surfaces
Cargo.toml, Cargo.lock, rust-toolchain.toml, src/main.rs, src/lib.rs, src/app.rs, README.md, .gitignore.

## Pass Or Fail Checks
고정 toolchain으로 fmt/check/clippy/test/release 성공; release EXE가 --version/--help/입력 없는 진단에서 정상 종료; Python 소스/config/assets 바이트 불변.

## Regression Surfaces
기존 Python 실행, config/assets 경로·내용, 별도 runtime의 독립성, 현재 실게임 승인/복구 상태. 모든 변경은 해당 feature 경계의 build·회귀·실패 정리 검사를 거친다.

## Harness Trace
- Active spec: [SPEC-0015](../spec/spec-0015-rust-native-build.md).
- Active run: [RUN-20261022-01](../run/run-20261022-01-rust-native-build.md).
- Execution profile: `infra-devtool`.
- Evaluation: [EVAL-0015](../evaluation/eval-0015-rust-native-build.md). source/static 검사 통과와 MSVC linker 부재로 실패한 실제 build/test gate를 구분한다. 구현 결함 fix log는 아직 해당 없음.

## Continuity Notes
- `2026-10-22`: 전체 실행 요청의 단계별 추적 경계로 작성. 첫 실행 대상으로 승인 범위를 고정했다.
- `2026-10-22`: Rust 설치·기반 소스·static 검사는 완료했으나 Microsoft 관리자 설치 단계 실패로 MSVC 링크가 불가능하다. UAC/직접 설치 선택을 질문하고 기술적으로 중단했다. 재개 권한 자체는 사용자의 전체 실행 위임을 유지한다.
- `2026-10-22`: 사용자 UAC 재요청 답변을 받아 같은 run의 Attempt2로 설치를 재시도한다. 아직 link/실행 통과를 뜻하지 않는다.
- `2026-10-22`: Attempt2에서 공식 Build Tools17.14.42/SDK22621 설치와4개 Rust test·release link·EXE help/version/diagnose/invalid-argument exit2를 통과했다. 이 foundation의 자동 검증 완료이며 전체 앱 이전/사람의 최종 전환 수용과 구분한다.
