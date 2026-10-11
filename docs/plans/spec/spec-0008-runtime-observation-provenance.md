# SPEC-0008: 단조 시각·세대별 실시간 관찰

- ID: `spec-0008`; Status: `implemented`; Run: `run-20261009-01`; Attempt: `1`
- Feature: [FEAT-0009](../feature/feat-0009-runtime-observation-provenance.md); Parent: [PRD-0008](../prd/prd-0008-realtime-runtime-and-recording-retention.md)
- Profile: `foundation-contract`; Evaluators: Contract, Functional; Created / Updated: `2026-10-09`

## Contract

`RuntimeFrame.captured_at`는 capture 시작 직전 단조 시각이다. `wall_captured_at`만 원본 packet의 일반 시각을 보존한다. vision의 source 시각/세대는 처리한 frame에서 그대로 복사하며 produced_at·sampled_at도 runtime에 주입한 같은 clock을 사용한다. capture 종료 시각으로 프레임 수명을 늘리지 않는다. 대기/종료도 같은 clock을 사용하고 capture는 finally에서 닫는다.

state는 단일 증가 generation과 최신 frame/최근 5개 frame, 각 vision channel의 마지막 source sequence를 소유한다. invalidate는 세대를 증가시키고 채택 상태를 지운다. capture는 시작 전 세대를 읽고 publication 시 다시 검사한다. 관찰 도중 invalidate되면 그 capture를 거부한다. 현재 세대의 새 capture에서 target rectangle이 바뀌면 state가 세대를 증가시키고 그 frame만 새 세대로 채택한다.

publication은 source generation·sequence·시각·완료 시각의 순서와 현재 시각을 검사한다. vision은 현재 frame보다 미래 source 또는 같은 channel의 중복/역순 source를 거부한다. payload=None은 그 channel의 이전 결과를 지우되 처리한 source 순번은 기억하여 더 오래된 결과가 복원되지 못하게 한다. decision은 결정에 사용한 snapshot의 source frame/세대를 갖고 invalidate 뒤 완료된 결과를 거부한다. clock 역행은 오류로 중단한다. 이 publication 검사는 callback 내부의 이미 전송된 입력을 취소했다는 주장이 아니다.

북쪽 소비자는 세대 변경 시 재사용 vision/프레임 변화 캐시를 버리고 최신 결과를 기다린다. 변화량 자체를 실제 도착/안전으로 승격하지 않는다. 후속 입력 소비자는 invalidate API를 이동/방/프로필 변경에 호출하고 행동 직전 현재 세대를 재확인해야 한다. 이번 기능은 그 API를 제공하며 완전 live 연결을 주장하지 않는다.

## Verification

명시적 fake clock/capture로 wall-clock 차이, 느린 capture, capture/vision/decision 도중 invalidate, 창 영역 변경, 역순 channel 결과, 관찰 없음 뒤 늦은 결과, 미래 시각, clock 역행과 최근 5개 상한을 검증한다. 변경 Python은 Black/compile, 기존 전체 unittest 회귀를 수행한다. 실제 게임 입력은 보내지 않는다.
