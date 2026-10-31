# FEAT-0022: 캐릭터 정책·관찰·필드 상태

## Metadata
- ID: `feat-0022`; Status: `passed`; Type: `foundation`; Surface: `backend`.
- Execution Profile: `foundation-contract`; Required Evaluators: Contract, Functional.
- Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md); Created: `2026-10-22`; Updated: `2026-10-23`.

## Goal
동일한 시간·관찰·프로필에서 생존/버프/이동 요청과 보류가 일치한다.

## Acceptance Contract
기존 정책/상태 테스트 대응, stale/future/duplicate/clock-regression/0.25초·deadline 경계, 이벤트/명령 trace 비교.

## Scope Boundary
- In: casting, survival, buffing, field control/provenance, held owner·독립 release·generation, 가짜 clock 입력.
- Out: 요청을 효과/처치/획득 확인으로 승격, main과 별도 runtime의 임의 통합.

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
FEAT-0017/0018; 실제 side effect는 FEAT-0021 계약 소비.

## Likely Affected Surfaces
src/common/{casting,survival,buffing,field_control,realtime,field_runtime}.rs, tests/.

## Pass Or Fail Checks
기존 정책/상태 테스트 대응, stale/future/duplicate/clock-regression/0.25초·deadline 경계, 이벤트/명령 trace 비교.

## Regression Surfaces
기존 Python 실행, config/assets 경로·내용, 별도 runtime의 독립성, 현재 실게임 승인/복구 상태. 모든 변경은 해당 feature 경계의 build·회귀·실패 정리 검사를 거친다.

## Harness Trace
- Active spec: [SPEC-0019](../spec/spec-0019-rust-character-policy.md); Active run: [RUN-20261022-05](../run/run-20261022-05-rust-character-policy.md).
- Execution profile: `foundation-contract`.
- Evaluation: [EVAL-0019](../evaluation/eval-0019-rust-character-policy.md), Contract/Functional PASS for SPEC-0019's input-free boundary.

## Continuity Notes
- `2026-10-22`: 전체 실행 요청의 단계별 추적 경계로 작성. 아직 실행하지 않았으며 하위 세부/불확실성은 실행 전 정리한다.
- `2026-10-22`: 설정/좌표 기반 통과 후 JPEG 결정과 독립인 입력 없는 상태/가짜 backend 경계를 고정해 실행한다. FEAT-0019는 blocked이며 이 feature의 prerequisite가 아니다. 실제 Win32 adapter/observations는 후속이다.
- `2026-10-23`: 정책·버프·필드 판단/held owner의 첫11개 Rust 검사가 통과했다. 버퍼/generation와 worker/mailbox 수명은 아직 구현 중이며 전체 feature 통과로 해석하지 않는다. 세부 실행 증거는 active run이 소유한다.
- `2026-10-23`: 상태/버프/입력 소유권·provenance·worker/mailbox·순수 HUD bridge의 입력 없는 경계를 완료했다.33개 신규 검사와 회귀/빌드 증거는 run/evaluation이 소유한다. 실제 Win32·이미지 판독·GUI·게임 검증과 전체 parity는 후속이다.
