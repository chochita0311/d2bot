# FEAT-0028: Python/OpenCV 없는 Windows 실행 배포

## Metadata
- ID: `feat-0028`; Status: `in-loop`; Type: `product`; Surface: `mixed`.
- Execution Profile: `fullstack-product`; Required Evaluators: Contract, Functional.
- Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md); Created: `2026-10-22`; Updated: `2026-10-24`.

## Goal
완성한 기능을 Windows EXE와 공용 설정·자산으로 실행한다.

## Acceptance Contract
모든 유지 경로 mapping/audit/회귀·Windows GUI/CLI/녹화·오류 통과, 의존성 없는 환경, distribution size/latency/중지 측정, 사용자 전환 수용.

## Scope Boundary
- In: release package, Python/OpenCV 없는 실제 실행, 전체 parity/의존성/성능·stop 측정, README/기본 실행 전환.
- Out: Python 소스/환경 삭제, 단일파일 내장 의무, 게임 전체 수용 결과 변경.

## Contract Surfaces
기존 Python의 해당 공개 동작/입출력과 [계획의 invariants](../refactoring/refactor-0002-rust-port.md#invariants)가 기준이다. Rust의 구현 타입·자원 소유권은 각 spec에서 명시하며, 모호한 값·fallback·운영 부작용을 추측하지 않는다.

## User-Visible Outcome
완성한 기능을 Windows EXE와 공용 설정·자산으로 실행한다.

## Entry And Exit
- Entry: 승인한 실제 호출/진단/테스트 경계에서 시작.
- Exit: 성공·보류·오류·취소를 구분하고 소유한 worker/handle/input을 정리한다.

## State Expectations
- 기본·정상·누락/불명·오류·취소 상태를 구분한다. 기존의 불명을 성공으로 바꾸지 않는다.
- 실제 게임 side effect는 해당 feature/spec의 명시한 경계 안에서만 검증한다.

## Dependencies
유지 대상 이전 feature 전체 완료. 전체 경로 검증용 최종 통합이며 기능 누락으로 게이트 축소 금지.

## Likely Affected Surfaces
release 배포 owner, README.md, docs/project/, docs/plans/, 전체 Rust 앱.

## Pass Or Fail Checks
모든 유지 경로 mapping/audit/회귀·Windows GUI/CLI/녹화·오류 통과, 의존성 없는 환경, distribution size/latency/중지 측정, 사용자 전환 수용.

## Regression Surfaces
기존 Python 실행, config/assets 경로·내용, 별도 runtime의 독립성, 현재 실게임 승인/복구 상태. 모든 변경은 해당 feature 경계의 build·회귀·실패 정리 검사를 거친다.

## Harness Trace
- Active spec: [SPEC-0028](../spec/spec-0028-rust-windows-distribution.md); Run: [RUN-20261023-09](../run/run-20261023-09-rust-windows-distribution.md). 전체 실행·실게임/속도 검사 위임 아래 선행 지정 경계를 마치고 최종 통합에 진입한다.
- Execution profile: `fullstack-product`.
- Evaluation: [EVAL-0031](../evaluation/eval-0031-rust-windows-distribution.md). 구현·자동 회귀·최종 복사 EXE의 GUI/CLI/녹화·제한한 실게임/성능 검사는 완료했다. 기본 실행 전환·외관의 사람 수용 대기이므로 `in-loop`를 유지하며, 전체 게임 성공이나 새 OS 검증을 PASS로 채우지 않는다.

## Continuity Notes
- `2026-10-24` 최종: Rust292+명시적 native4, Python765 검사가 통과했다. `dist/d2bot-windows-x64`에 최종 EXE와 설정/공용 자산·고지를 제공하고 모든922 payload 해시를 확인했다. 실제 북쪽 이동·근접 전투 입력·소환술사 플랫폼 인식을 관찰했으며 모든 F2/F4 눌림은 해제됐다. 전체 처치/키/방 완주는 미확인, 느린 관찰 거부는 유지한다. 종료 후 Act2 캐릭터 선택 화면을 확인했다. 세부 수치·오류·메모리 한계는 EVAL-0031이 소유한다. 사람의 기본 전환 수용은 아직 수신하지 않았다.
- `2026-10-24`: Python765 및 최초 Rust285 회귀, Python lookup 없는 GUI/진단·실제 PNG/영상·방 진입/버프 입력을 확인했다. 연속 이동68개 관찰은 모두250ms를 넘겨 입력을 거부했으므로 필드 성능은 미통과다. 기존 시한을 유지하고 영상 계산 병목을 수정·회귀·재측정한다. 사람의 복구/Act1 저장 종료 확인은 수신 완료이며 반복 요청하지 않는다.
- `2026-10-23`: 모든 앞선 지정 기능 경계를 통과했다. 배포·전체 회귀·의존성·새 화면/실게임/성능 증거를 실제로 수집하며 최종 사람 전환 수용은 구분한다.
- `2026-10-22`: 전체 실행 요청의 단계별 추적 경계로 작성. 아직 실행하지 않았으며 하위 세부/불확실성은 실행 전 정리한다.
