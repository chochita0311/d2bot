# FEAT-0017: 기존 JSON 설정과 선택 계약

## Metadata
- ID: `feat-0017`; Status: `passed`; Type: `foundation`; Surface: `backend`.
- Execution Profile: `foundation-contract`; Required Evaluators: Contract, Functional.
- Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md); Created/Updated: `2026-10-22`.

## Goal
Rust 설정 생산자가 기존 JSON 병합·기본값·선택 의미를 그대로 제공한다.

## Acceptance Contract
정상 자료의 Python/Rust 구조·선택·거부 결과가 일치; bool/int·null/누락·병합 경계 회귀 통과. 사용자 승인 delta: 잘못된 선택적 버프 대기시간은 소비자 기본값으로 복귀하고 경고하며 설정 로드를 계속한다.

## Scope Boundary
- In: UTF8 BOM, 단일/재귀 JSON, 정렬/삽입 순서, 재귀 병합, 타입/null·에러·캐릭터/프로필/farm 선택, 공유 경로.
- Out: 설정 키 정리/자동 변경, 새 기본값, UI 설정 편집.

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
FEAT-0016의 빌드 환경.

## Likely Affected Surfaces
src/common/config.rs, diablo2/common/config.py(비교), config/, tests/.

## Pass Or Fail Checks
동일 자료의 Python/Rust 구조·선택·거부 결과가 일치; bool/int·null/누락·병합 경계 회귀 통과.

## Regression Surfaces
기존 Python 실행, config/assets 경로·내용, 별도 runtime의 독립성, 현재 실게임 승인/복구 상태. 모든 변경은 해당 feature 경계의 build·회귀·실패 정리 검사를 거친다.

## Harness Trace
- Active spec: [SPEC-0016](../spec/spec-0016-rust-config-contract.md); Active run: [RUN-20261022-02](../run/run-20261022-02-rust-config-contract.md). 사용자의 전체 실행 위임 아래 FEAT-0016 통과 후 입력 없는 경계를 고정했다.
- Execution profile: `foundation-contract`.
- Evaluation: [EVAL-0016](../evaluation/eval-0016-rust-config-contract.md). 10개 설정 비교/보정 검사와 전체14개 Rust 검사, fmt/check/clippy/release 및 oracle Black/compile을 통과했다. 전체 runtime parity는 후속이다.

## Continuity Notes
- `2026-10-22`: 전체 실행 요청의 단계별 추적 경계로 작성. 아직 실행하지 않았으며 하위 세부/불확실성은 실행 전 정리한다.
- `2026-10-22`: source/consumer/schema 조사 후 SPEC-0016의 문서·선택·검증 경계를 승인 범위로 고정하고 실행한다. legacy 약한 필드의 typed runtime adapter는 후속 소비자 경계이며 여기서 미승인 타입 강화를 하지 않는다.
- `2026-10-22`: NaN/무한대는 synthetic 비교 입력에서 확인했고 실제 개인 JSON에는 없었다. Python `float()`의 특수값·overflow 허용이 원인이며 사용자는 오류 종료 대신 보정을 요청했다. 잘못된 pause override를 제거하고 기존 missing-key 기본값을 사용하며 원본 경로/값/조치를 경고로 제공한다.
