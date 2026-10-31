# FEAT-0021: Windows 캡처와 중단 가능한 입력

## Metadata
- ID: `feat-0021`; Status: `passed`; Type: `foundation`; Surface: `backend`.
- Execution Profile: `foundation-contract`; Required Evaluators: Contract, Functional.
- Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md); Created: `2026-10-22`; Updated: `2026-10-23`.

## Goal
표준 OS 연동이 프레임 소유권·held 입력의 독립 해제 계약을 제공한다.

## Acceptance Contract
fake backend 실패/취소/지연·창 기하 변경·해제 검증; 입력 없는 실제 capture; 감독 입력은 기존 복구 전제 충족 후 별도 실제 증거.

## Scope Boundary
- In: 현재 capture mode/fallback/DPI/창/client, SendInput/readback, stop/focus/hotkey, deadline·취소·RAII 정리·제한한 unsafe 경계.
- Out: process/game hook, 새 capture API, 실게임 재개 승인, 무단 retry.

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
FEAT-0017/0018/0022 and compared normalization operations from FEAT-0019. JPEG file decoding is not used by native BGRA capture; its earlier separate gate subsequently passed under the user-approved reference-only exception.

## Likely Affected Surfaces
src/common/capture.rs, controller.rs, movement.rs, platform/, tests/.

## Pass Or Fail Checks
fake backend 실패/취소/지연·창 기하 변경·해제 검증; 입력 없는 실제 capture; 감독 입력은 기존 복구 전제 충족 후 별도 실제 증거.

## Regression Surfaces
기존 Python 실행, config/assets 경로·내용, 별도 runtime의 독립성, 현재 실게임 승인/복구 상태. 모든 변경은 해당 feature 경계의 build·회귀·실패 정리 검사를 거친다.

## Harness Trace
- Active spec: [SPEC-0020](../spec/spec-0020-rust-windows-io.md); Active run: [RUN-20261023-01](../run/run-20261023-01-rust-windows-io.md).
- Execution profile: `foundation-contract`.
- Evaluation: [EVAL-0020](../evaluation/eval-0020-rust-windows-io.md), Contract/Functional PASS for the input-free SPEC-0020 boundary.

## Continuity Notes
- `2026-10-22`: 전체 실행 요청의 단계별 추적 경계로 작성. 아직 실행하지 않았으며 하위 세부/불확실성은 실행 전 정리한다.
- `2026-10-23`: FEAT-0022 통과 뒤 표준 Win32 adapter의 소유권/실패 경계를 확정해 실행한다. 검증은 가짜 입력과 실제 입력 없는 캡처에 한정하며 게임 복구 전제를 해제하지 않는다.
- `2026-10-23`:12개 가짜/비교 검사와4개 명시적 native 검사, fmt/Clippy/release를 통과했다. 자체16px 시험 창은 포커스를 유지하고 두 색/BGR·위아래 방향을 확인한 뒤 제거한다. 실제 게임 입력, 전체 앱/UI 통합과 중단 키의 별도 제안 변경은 이 통과에 포함하지 않는다.
