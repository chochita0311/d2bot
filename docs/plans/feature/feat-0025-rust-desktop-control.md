# FEAT-0025: 기존 조작 흐름의 Rust 제어판

## Metadata
- ID: `feat-0025`; Status: `passed`; Type: `product`; Surface: `frontend`.
- Execution Profile: `frontend-product`; Required Evaluators: Contract, Functional, Design, UX.
- Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md); Created/Updated: `2026-10-22`.

## Goal
기존 선택·시작·중지·로그·상태 동작을 Rust native GUI로 제공한다.

## Acceptance Contract
GUI 실제 render/직접 관찰, 기존 GUI mock 사례+연결 검증, 느린 worker 중 응답/stop/close·메모리 상한 확인.

## Scope Boundary
- In: egui/eframe 후보 검증 후 기존 배치 의도/크기/한글/DPI/캡처후보·액션 배타·worker 이벤트/닫기.
- Out: 화면 재설계, 모든 mode dry/pause 통합, 확인 안 된 action 성공 표시.

## Contract Surfaces
기존 Python의 해당 공개 동작/입출력과 [계획의 invariants](../refactoring/refactor-0002-rust-port.md#invariants)가 기준이다. Rust의 구현 타입·자원 소유권은 각 spec에서 명시하며, 모호한 값·fallback·운영 부작용을 추측하지 않는다.

## User-Visible Outcome
기존 선택·시작·중지·로그·상태 동작을 Rust native GUI로 제공한다.

## Entry And Exit
- Entry: 승인한 실제 호출/진단/테스트 경계에서 시작.
- Exit: 성공·보류·오류·취소를 구분하고 소유한 worker/handle/input을 정리한다.

## State Expectations
- 기본·정상·누락/불명·오류·취소 상태를 구분한다. 기존의 불명을 성공으로 바꾸지 않는다.
- 실제 게임 side effect는 해당 feature/spec의 명시한 경계 안에서만 검증한다.

## Dependencies
선택한 GUI framework 검증과 FEAT-0023/0024/0026. FEAT-0026은 최종 녹화 연결의 의존성이며 독립 화면/상태/기존 action 연결은 먼저 진행한다. UI placeholder를 완성 기능으로 수용하지 않음.

## Likely Affected Surfaces
src/ui/, src/app.rs, tests/.

## Pass Or Fail Checks
GUI 실제 render/직접 관찰, 기존 GUI mock 사례+연결 검증, 느린 worker 중 응답/stop/close·메모리 상한 확인.

## Regression Surfaces
기존 Python 실행, config/assets 경로·내용, 별도 runtime의 독립성, 현재 실게임 승인/복구 상태. 모든 변경은 해당 feature 경계의 build·회귀·실패 정리 검사를 거친다.

## Harness Trace
- Active spec: [SPEC-0024](../spec/spec-0024-rust-desktop-control.md); Run: [RUN-20261023-05](../run/run-20261023-05-rust-desktop-control.md). 전체 실행 위임 아래 실제 renderer/계약 비교를 진행하며 녹화 외부 의존성은 별도 확인한다.
- Execution profile: `frontend-product`.
- Evaluation: [EVAL-0029](../evaluation/eval-0029-rust-desktop-control.md), 승인한 fake-effects/native-render 경계 PASS. 실제 게임·전체 진입점·최종 사용자 전환 검토는 FEAT-0027/0028에서 이어간다.

## Continuity Notes
- `2026-10-23`: Rust MPEG-4 연결, 실제 Tkinter/Rust 표시 대조, DPI/하단 접근성/비동기 정리 수정 및 지정 검사 완료. 전체 포팅은 계속한다.
- `2026-10-23`: 순수 표시와 실제 세션 소유권·native adapter·host를 구현했다. 최종 녹화 연결을 위해 FEAT-0026을 먼저 실행하고 이후 같은 GUI run을 재개한다. 현재 blocked는 녹화 의존성 경계이며 전체 작업 중단이 아니다.
- `2026-10-22`: 전체 실행 요청의 단계별 추적 경계로 작성. 아직 실행하지 않았으며 하위 세부/불확실성은 실행 전 정리한다.
