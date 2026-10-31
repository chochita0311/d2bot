# FEAT-0027: CLI preview·관찰·재생 도구

## Metadata
- ID: `feat-0027`; Status: `passed`; Type: `product`; Surface: `backend`.
- Execution Profile: `backend-product`; Required Evaluators: Contract, Functional.
- Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md); Created/Updated: `2026-10-22`.

## Goal
유지 대상 CLI와 입력 없는 replay를 Rust 명령으로 실행한다.

## Acceptance Contract
기존 명령→Rust 명령 대응과 오류/출력/입력 trace; GUI와 별도 hotkey/dry-run 의미 비교.

## Scope Boundary
- In: 기존 parser 인자/배타·help/오류/exit, core bot overlay preview, 각 tools bin·재생 출력.
- Retained tool mapping: `diablo2.actions.town_loop_vision`의 `calibrate`/`calibrate_scene` 자산 writer와 JSON state/frame 기반 한 번의 planner-step 명령은 이 feature에서 이전한다. FEAT-0023의 실행 플래너/판독 기반을 재사용하며 CLI/asset 갱신 인자·오류·출력을 따로 비교한다.
- Out: 기존 flag 정리/이름 변경, synthetic replay를 실제 증거로 취급.

## Contract Surfaces
기존 Python의 해당 공개 동작/입출력과 [계획의 invariants](../refactoring/refactor-0002-rust-port.md#invariants)가 기준이다. Rust의 구현 타입·자원 소유권은 각 spec에서 명시하며, 모호한 값·fallback·운영 부작용을 추측하지 않는다.

## User-Visible Outcome
유지 대상 CLI와 입력 없는 replay를 Rust 명령으로 실행한다.

## Entry And Exit
- Entry: 승인한 실제 호출/진단/테스트 경계에서 시작.
- Exit: 성공·보류·오류·취소를 구분하고 소유한 worker/handle/input을 정리한다.

## State Expectations
- 기본·정상·누락/불명·오류·취소 상태를 구분한다. 기존의 불명을 성공으로 바꾸지 않는다.
- 실제 게임 side effect는 해당 feature/spec의 명시한 경계 안에서만 검증한다.

## Dependencies
각 사용 계약 FEAT-0017~0026 중 필요한 것; bin별 좁은 연결 검증.

## Likely Affected Surfaces
src/app.rs, src/core/, src/tools/, src/bin/, tests/.

## Pass Or Fail Checks
기존 명령→Rust 명령 대응과 오류/출력/입력 trace; GUI와 별도 hotkey/dry-run 의미 비교.

## Regression Surfaces
기존 Python 실행, config/assets 경로·내용, 별도 runtime의 독립성, 현재 실게임 승인/복구 상태. 모든 변경은 해당 feature 경계의 build·회귀·실패 정리 검사를 거친다.

## Harness Trace
- Active spec: [SPEC-0027](../spec/spec-0027-rust-cli-replay.md); Run: [RUN-20261023-08](../run/run-20261023-08-rust-cli-replay.md). 전체 실행 위임 아래 기존 CLI/도구 계약을 확정하고 실행한다.
- Execution profile: `backend-product`.
- Evaluation: [EVAL-0030](../evaluation/eval-0030-rust-cli-replay.md). Source/stored/fake·native preview 연결 경계를 통과했으며 실제 게임·전체 배포 수용은 FEAT-0028에 남긴다.

## Continuity Notes
- `2026-10-23`: 실제 진입점/도구/preview와 종료·오류 연결을 검증했다. 배포·전체 회귀·실게임 지연/중단과 최종 사람 전환 수용은 후속이며 이 scoped PASS로 대체하지 않는다.
- `2026-10-23`: GUI·직접 Rust MPEG-4의 지정 검증을 마치고 단일 EXE 진입점과 기존 CLI/도구 연결을 시작한다. 실제 게임 검증은 전체 연결 뒤 FEAT-0028에서 수행한다.
- `2026-10-22`: 전체 실행 요청의 단계별 추적 경계로 작성. 아직 실행하지 않았으며 하위 세부/불확실성은 실행 전 정리한다.
