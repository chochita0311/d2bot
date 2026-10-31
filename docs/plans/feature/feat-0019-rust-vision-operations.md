# FEAT-0019: 프로젝트가 쓰는 영상 연산

## Metadata
- ID: `feat-0019`; Status: `passed`; Type: `foundation`; Surface: `backend`.
- Execution Profile: `foundation-contract`; Required Evaluators: Contract, Functional.
- Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md); Created/Updated: `2026-10-22`.

## Goal
필요한 영상 연산을 OpenCV/NumPy 없이 같은 판정 계약으로 제공한다.

## Acceptance Contract
고정 자료의 수치 오차를 사전 지정하고 최종 탐지/거부 차이0; malformed/상수/경계·threshold 비교.

## Scope Boundary
- In: 이미지 IO, 색변환/resize/필터/마스크/형태학/components/distance, 채널별 평균 제거 template matching, score/box/동점.
- Out: OpenCV 전체 구현, 임계값 완화, 무단 알고리즘 교체.

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
FEAT-0018와 고위험 template 비교.

## Likely Affected Surfaces
src/common/vision_ops/, src/common/detectors.rs, tests/.

## Pass Or Fail Checks
고정 자료의 수치 오차를 사전 지정하고 최종 탐지/거부 차이0; malformed/상수/경계·threshold 비교.

## Regression Surfaces
기존 Python 실행, config/assets 경로·내용, 별도 runtime의 독립성, 현재 실게임 승인/복구 상태. 모든 변경은 해당 feature 경계의 build·회귀·실패 정리 검사를 거친다.

## Harness Trace
- Active spec: [SPEC-0018](../spec/spec-0018-rust-vision-operations.md); Active run: [RUN-20261022-04](../run/run-20261022-04-rust-vision-operations.md).
- Execution profile: `foundation-contract`.
- Evaluation: [EVAL-0018](../evaluation/eval-0018-rust-vision-operations.md). 사용자가 JPEG 참고 화면의 측정 차이를 허용한 뒤 수정한 계약으로 release 영상 검사15개가 filter 없이 통과했다. 기존 JPEG 실패 이력과1056개 lossless asset 비교는 evaluation에 유지한다. 이 한정된 foundation 통과가 전체 앱 동등성이나 실제 게임 실행을 입증하지 않는다.

## Continuity Notes
- `2026-10-22`: 전체 실행 요청의 단계별 추적 경계로 작성. 아직 실행하지 않았으며 하위 세부/불확실성은 실행 전 정리한다.
- `2026-10-22`: 실제 common/Summoner/actions primitive와 asset dtype/channel inventory를 조사했고 FEAT-0018 통과 후 이 경계를 고정했다. 소비자별 판단/manifest는 후속 feature에 남기고 공용 수치 연산과 decoding/normalization을 구현한다.
