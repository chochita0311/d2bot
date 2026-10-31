# FEAT-0020: 기존 ORB 기반 배경 이동 추정

## Metadata
- ID: `feat-0020`; Status: `passed`; Type: `foundation`; Surface: `backend`.
- Execution Profile: `foundation-contract`; Required Evaluators: Contract, Functional.
- Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md); Created: `2026-10-22`; Updated: `2026-10-23`.

## Goal
프레임 간 이동량과 실패/거부 의미를 순수 Rust로 재현한다.

## Acceptance Contract
동일 synthetic/실제 연속 frame의 중간 결과·이동량·실패사유 비교; seed와 허용 오차 고정; 미감사 조건은 provisional.

## Scope Boundary
- In: ORB600/descriptor, Hamming KNN2/0.7 strict ratio, partial-affine RANSAC, inlier/선형오차/translation 부호·순서/시계 검증.
- Out: 다른 tracker 대체, acceptance 조건 완화, 실시간 main 통합.

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
FEAT-0018 and compared image operations in FEAT-0019; ORB/RANSAC is verified against the installed baseline. Native frames/PNG did not depend on the earlier JPEG gate, which subsequently passed under its explicitly approved reference-only exception.

## Likely Affected Surfaces
src/common/scene_motion.rs, tests/.

## Pass Or Fail Checks
동일 synthetic/실제 연속 frame의 중간 결과·이동량·실패사유 비교; seed와 허용 오차 고정; 미감사 조건은 provisional.

## Regression Surfaces
기존 Python 실행, config/assets 경로·내용, 별도 runtime의 독립성, 현재 실게임 승인/복구 상태. 모든 변경은 해당 feature 경계의 build·회귀·실패 정리 검사를 거친다.

## Harness Trace
- Active spec: [SPEC-0021](../spec/spec-0021-rust-background-motion.md); Active run: [RUN-20261023-02](../run/run-20261023-02-rust-background-motion.md).
- Execution profile: `foundation-contract`.
- Evaluation: [EVAL-0021](../evaluation/eval-0021-rust-background-motion.md), Contract/Functional PASS for the input-free foundation. Main/GUI/navigation integration and full-port parity remain outside this result.

## Continuity Notes
- `2026-10-22`: 전체 실행 요청의 단계별 추적 경계로 작성. 아직 실행하지 않았으며 하위 세부/불확실성은 실행 전 정리한다.
- `2026-10-23`: Windows 입력 없는 기반 통과 후 ORB/상태/매칭/affine의 비교 경계를 고정해 실행한다. 기존 OpenCV4.13.0 알고리즘의 Rust 재구현을 조사하며 native OpenCV dependency나 다른 tracker 대체는 허용하지 않는다.
- `2026-10-23`: 고정한 중간 결과/수치/상태 비교와 실제 기존 PNG 두 장, 한정 정적 검사·release 빌드를 통과했다. 설정/자산과 기존 실게임 복구 조건은 유지하고 다음 action 경계로 진행한다.
