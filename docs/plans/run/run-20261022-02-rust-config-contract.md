# RUN-20261022-02: Rust 설정 계약 이전

## Metadata
- ID: `run-20261022-02`; Status: `passed`; Attempt: `1`.
- Feature: [FEAT-0017](../feature/feat-0017-rust-config-contract.md); Parent PRD: [PRD-0009](../prd/prd-0009-rust-windows-port.md).
- Active Spec: [SPEC-0016](../spec/spec-0016-rust-config-contract.md); Profile: `foundation-contract`; Evaluators: Contract, Functional; Created/Updated: `2026-10-22`.

## Goal
같은 JSON의 기본값/검증/선택을 Python과 비교해 Rust 공용 설정 경계를 제공한다. 실제 입력/GUI는 실행하지 않는다.

## Selected Loop
Orchestrator가 single backend foundation과 Contract/Functional을 선택했다. FEAT-0016의 native build/4tests/release smoke가 prerequisite를 충족했다. SPEC-0016에서 raw/defaulted legacy 문서와 검증하는 policy schema, runtime typed adapter의 후속 책임을 구분했다.

## Contract Surfaces
정렬 재귀/BOM/병합/필드/기본값·오류, 캐릭터·farm 순서/선택/참조, snapshot. 실제 config/assets를 공유하며 새 복제 fixture를 만들지 않는다.

## Current Artifacts
SPEC-0016, Rust config/schema source, 10개 비교/보정 tests와 개발 전용 Python oracle. [EVAL-0016](../evaluation/eval-0016-rust-config-contract.md)이 source 계약과 실제 검사를 소유한다.

## Evaluation Coverage
Contract/Functional `PASS`, 이 spec의 요구 증거 `complete`: 10개 설정 test와 전체14개 Rust test/0실패, fmt/check/clippy/release/Black/compile 통과. 전체 임의 malformed 입력 동등성·상태 실행·실게임은 이 증거 범위 밖이다.

## Current Route
현재 blocker 없음. 다음 독립 프레임/좌표 feature의 계약을 고정한다. 전체 동등성은 Unknown/Provisional을 유지한다.

## Attempts
- Attempt1: Python source 모델/선택·실제 consumer mutation과 기존 테스트의 누락된 loader/BOM/merge 사례를 확인했다. spec을 고정한 뒤 implementation과 oracle 비교를 진행한다.
- 최초 NaN 비교는 Rust의 시작 오류와 Python의 특수 float 허용 차이로 실패했다. 실제 개인 JSON에서는 발견되지 않았다. 사용자에게 원인/변경을 질문했고, 오류 종료 대신 보정·실행 지속 지시를 받았다. planning/spec의 승인 delta로 기록한 뒤 잘못된 optional override를 제거하고 기존 consumer 기본값으로 복귀·경고하는 검사를 분리했다. 정상 자료의 비교 기준을 완화하지 않았다.
- 검증 중 Python3.7 예외 메시지/validation stage 순서·null constructor 메시지를 수정했다. Clippy let_and_return 1건을 수정한 뒤 고정 source로 전체 packet을 다시 수행했다.
- 최종 fmt/check/clippy/test/release 성공: library4+config10=14 tests/0실패, oracle Black/compile 통과. sandbox incremental hardlink 미지원 경고는 copy fallback이며 실패가 아니다.

## Post-Contract Regression Check
기존 Python runtime/config JSON/assets에는 변경이 없다. 캐릭터 설정 안내 Markdown만 승인 delta를 추가했다. oracle은 integration test의 subprocess이며 library/release entry에는 Python import/process 호출이 없다. 필수 정책의 오류·farm 원본 참조/선택·키 순서와 기본값을 비교했다.

## Human Review Outcome
전체 실행 위임 아래 이 foundation의 자동 검증이 완료됐다. 잘못된 pause 보정은 사용자 직접 지시에 따랐다. 후속 GUI/CLI의 경고 표시·runtime 시간 checked conversion과 최종 앱 전환 수용은 남아 있다.
