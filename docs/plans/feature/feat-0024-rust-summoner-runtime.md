# FEAT-0024: 현재 Summoner 실행 흐름

## Metadata
- ID: `feat-0024`; Status: `passed`; Type: `product`; Surface: `backend`.
- Execution Profile: `backend-product`; Required Evaluators: Contract, Functional.
- Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md); Created: `2026-10-22`; Updated: `2026-10-23`.

## Goal
현재 main의 실행 순서·관찰·판단·입력 계획을 Rust에서 재현한다.

## Acceptance Contract
기존 전체 흐름·실패·stale/focus/stop/resource·반복 거부를 결정적 trace로 비교; 실게임 통과는 별도 증거.

## Scope Boundary
- In: 현재 GUI session→main/run_once→continuous navigation/encounter/canyon/lifecycle, 독립 replay/runtime 보존, 결과/보고서.
- Out: 전체 미완료 gameplay 목표를 포팅 성공으로 처리, legacy north executor 복원, 새 알고리즘.

## Contract Surfaces
기존 Python의 해당 공개 동작/입출력과 [계획의 invariants](../refactoring/refactor-0002-rust-port.md#invariants)가 기준이다. Rust의 구현 타입·자원 소유권은 각 spec에서 명시하며, 모호한 값·fallback·운영 부작용을 추측하지 않는다.

## User-Visible Outcome
현재 main의 실행 순서·관찰·판단·입력 계획을 Rust에서 재현한다.

## Entry And Exit
- Entry: 승인한 실제 호출/진단/테스트 경계에서 시작.
- Exit: 성공·보류·오류·취소를 구분하고 소유한 worker/handle/input을 정리한다.

## State Expectations
- 기본·정상·누락/불명·오류·취소 상태를 구분한다. 기존의 불명을 성공으로 바꾸지 않는다.
- 실제 게임 side effect는 해당 feature/spec의 명시한 경계 안에서만 검증한다.

## Dependencies
FEAT-0019/0020/0021/0022/0023. 깊은 의존 체인은 기존 전체 흐름 이전에 필요하며 feature 내부를 검증 가능한 batch로 제한.

## Likely Affected Surfaces
src/runs/, src/common/각 판독기, tests/.

## Pass Or Fail Checks
기존 전체 흐름·실패·stale/focus/stop/resource·반복 거부를 결정적 trace로 비교; 실게임 통과는 별도 증거.

## Regression Surfaces
기존 Python 실행, config/assets 경로·내용, 별도 runtime의 독립성, 현재 실게임 승인/복구 상태. 모든 변경은 해당 feature 경계의 build·회귀·실패 정리 검사를 거친다.

## Harness Trace
- Active spec: [SPEC-0023](../spec/spec-0023-rust-summoner-runtime.md); Active run: [RUN-20261023-04](../run/run-20261023-04-rust-summoner-runtime.md). 기존 전체 실행 위임에 따라 관찰→상태→가짜 입력→현재 main 순서로 비교한다.
- Execution profile: `backend-product`.
- Evaluation: [EVAL-0027](../evaluation/eval-0027-rust-summoner-runtime.md), Contract/Functional PASS in the stored-image/fake-input/partial-initialization boundary. 실제 게임 성공·GUI/CLI/녹화·최종 배포 수용은 포함하지 않는다.

## Continuity Notes
- `2026-10-22`: 전체 실행 요청의 단계별 추적 경계로 작성. 아직 실행하지 않았으며 하위 세부/불확실성은 실행 전 정리한다.
- `2026-10-23`: 공통 HUD/무기/자동지도/가림, 장면·계단·발 위치·카메라·중앙, 목표/일지/종착점과 보스 외형/생사 관찰기를 구현하고 입력 없는 비교를 진행했다. 전체 Summoner main/경로/전투 흐름과 FEAT 완료 평가는 아직 남아 있다. 구체적인 비교 범위와 결과는 활성 run이 소유한다.
- `2026-10-23`: 현재 main·독립 search/runtime/cycle·공개 정의/GUI 세션까지 이전하고 한정된 자동 평가를 통과했다. 전체 Rust release252개 통과/4 native 제외, 입력957개 해시 일치. 기존 전체 실행 위임에 따라 남은 feature로 진행하며 최종 애플리케이션 완료로 취급하지 않는다.
