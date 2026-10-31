# FEAT-0018: 분석 프레임과 물리 좌표 계약

## Metadata
- ID: `feat-0018`; Status: `passed`; Type: `foundation`; Surface: `backend`.
- Execution Profile: `foundation-contract`; Required Evaluators: Contract, Functional.
- Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md); Created/Updated: `2026-10-22`.

## Goal
Rust 프레임·좌표 타입이 기존 기하·정밀도·기하 변경 거부를 표현한다.

## Acceptance Contract
기존 geometry 사례·음수 원점·±0.5·비율/기하 변경·잘못된 버퍼를 비교; 프레임은 소유권 없이 재활용되지 않음.

## Scope Boundary
- In: BGR/크기/stride 소유 프레임·borrowed crop, layout/target/client, 픽셀 중심·ties-even·역변환·aspect 계약과 native 판별.
- 정규화의 실제 resize/보간 픽셀 계산은 FEAT-0019가 이 프레임/기하 타입 위에서 구현한다. placeholder normalize를 제공하지 않는다.
- Out: 새 해상도 지원 주장, DPI 보정 추측, unsafe 버퍼 공유.

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
FEAT-0016의 빌드·FEAT-0017의 JSON 의존성. ReferenceLayout은 Python처럼 자체 JSON을 직접 읽는다. 픽셀 재표본화는 이 타입을 소비하는 FEAT-0019의 책임이다.

## Likely Affected Surfaces
src/common/frame_geometry.rs, src/common/frame.rs, tests/.

## Pass Or Fail Checks
기존 geometry 사례·음수 원점·±0.5·비율/기하 변경·잘못된 버퍼를 비교; 프레임은 소유권 없이 재활용되지 않음.

## Regression Surfaces
기존 Python 실행, config/assets 경로·내용, 별도 runtime의 독립성, 현재 실게임 승인/복구 상태. 모든 변경은 해당 feature 경계의 build·회귀·실패 정리 검사를 거친다.

## Harness Trace
- Active spec: [SPEC-0017](../spec/spec-0017-rust-frame-geometry.md); Active run: [RUN-20261022-03](../run/run-20261022-03-rust-frame-geometry.md).
- Execution profile: `foundation-contract`.
- Evaluation: [EVAL-0017](../evaluation/eval-0017-rust-frame-geometry.md). 7개 프레임/기하 비교와 전체21개 Rust 검사·fmt/check/clippy/release·oracle Black/compile 통과. 재표본화와 실제 OS 입력은 후속이다.

## Continuity Notes
- `2026-10-22`: 전체 실행 요청의 단계별 추적 경계로 작성. 아직 실행하지 않았으며 하위 세부/불확실성은 실행 전 정리한다.
- `2026-10-22`: 실제 Python 좌표/소비자·기존 테스트 조사 후 소유 BGR/borrowed view와 순수 기하로 경계를 고정했다. resize 구현은 FEAT-0019에 모아 의존성 순환을 피한다. 전체 실행 위임 아래 이 foundation을 진행한다.
