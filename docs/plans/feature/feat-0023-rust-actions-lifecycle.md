# FEAT-0023: 방 lifecycle·조합·일반 줍기

## Metadata
- ID: `feat-0023`; Status: `passed`; Type: `product`; Surface: `backend`.
- Execution Profile: `backend-product`; Required Evaluators: Contract, Functional.
- Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md); Created: `2026-10-22`; Updated: `2026-10-23`.

## Goal
기존 독립 action을 Rust 세션으로 시작·취소하고 같은 결과를 관찰한다.

## Acceptance Contract
고정 화면/가짜 backend로 입력·상태·부분 실패·출력 비교; 시작/중지와 자원 종료 확인.

## Scope Boundary
- In: town/room create-exit, gem/loot 관찰·세션, 오류·취소·반복·증거 writer.
- Out: 새 hunting 정책, 자동 복구/정비 구현, 범용 recorder redesign.

## Contract Surfaces
기존 Python의 해당 공개 동작/입출력과 [계획의 invariants](../refactoring/refactor-0002-rust-port.md#invariants)가 기준이다. Rust의 구현 타입·자원 소유권은 각 spec에서 명시하며, 모호한 값·fallback·운영 부작용을 추측하지 않는다.

## User-Visible Outcome
기존 독립 action을 Rust 세션으로 시작·취소하고 같은 결과를 관찰한다.

## Entry And Exit
- Entry: 승인한 실제 호출/진단/테스트 경계에서 시작.
- Exit: 성공·보류·오류·취소를 구분하고 소유한 worker/handle/input을 정리한다.

## State Expectations
- 기본·정상·누락/불명·오류·취소 상태를 구분한다. 기존의 불명을 성공으로 바꾸지 않는다.
- 실제 게임 side effect는 해당 feature/spec의 명시한 경계 안에서만 검증한다.

## Dependencies
FEAT-0019/0021/0022, 각 화면 판독 비교 선행.

## Likely Affected Surfaces
src/actions/, src/common/해당 vision, tests/.

## Pass Or Fail Checks
고정 화면/가짜 backend로 입력·상태·부분 실패·출력 비교; 시작/중지와 자원 종료 확인.

## Regression Surfaces
기존 Python 실행, config/assets 경로·내용, 별도 runtime의 독립성, 현재 실게임 승인/복구 상태. 모든 변경은 해당 feature 경계의 build·회귀·실패 정리 검사를 거친다.

## Harness Trace
- Active spec: [SPEC-0022](../spec/spec-0022-rust-actions-lifecycle.md); Active run: [RUN-20261023-03](../run/run-20261023-03-rust-actions-lifecycle.md). Existing full-port delegation authorizes this bounded execution.
- Execution profile: `backend-product`.
- Evaluation: [EVAL-0022](../evaluation/eval-0022-rust-actions-lifecycle.md). 입력 없는/가짜 입력 계약의 전체 Rust125개·별도 release29개 검사와 release build를 통과했다. 네이티브 입력·GUI·Summoner/완성 앱의 증거로 확대하지 않는다.

## Continuity Notes
- `2026-10-22`: 전체 실행 요청의 단계별 추적 경계로 작성. 아직 실행하지 않았으며 하위 세부/불확실성은 실행 전 정리한다.
- `2026-10-23`: ORB 기반 통과 후 독립 action 계약을 실행한다. JPEG 종속 비교는 별도 결정 대기를 유지하며 입력 없는 단계부터 진행한다.
