# FEAT-0009: 실시간 관찰 시각과 이동 세대

## Metadata

- ID: `feat-0009`; Status: `passed`; Type: `foundation`
- Parent PRD: [PRD-0008](../prd/prd-0008-realtime-runtime-and-recording-retention.md)
- Surface / Execution Profile: Python 실시간 관찰 생산자·북쪽 소비자 / `foundation-contract`
- Required Evaluators: Contract, Functional
- Created / Updated: `2026-10-09`
- Approval: 사용자가 요구사항 확정·구현·감독 검증까지 진행하도록 명시했다. 이 기능은 이미 확정한 최신 관찰·단일 시계·과거 이동 결과 거부의 구현이며 미확정 live 시간/반경을 결정하지 않는다.

## Goal And Boundary

기존 북쪽 비전 runtime이 공통 필드 제어와 같은 단조 시각을 생산하고, 창 영역 변경 또는 호출자의 이동 무효화 뒤 이전 인식/판단 결과를 다시 채택하지 않도록 한다. 새로운 화면마다 전체 worker를 만들지 않고 기존 capture/fast/slow/decision worker와 최근 5개 버퍼를 유지한다.

포함: 관찰 시작/인식 완료/판단 시각의 같은 시계, 원본 wall-clock 시각의 기록용 분리, 명시적 관찰 세대와 늦은 publication 거부, 최신성의 보수적 시작 시각, 북쪽 캐시의 세대 분리, 입력 없는 동시성 회귀.

제외: 자동 몬스터 없음/도착/죽음/획득 판독, 새 전투 반경·운영 시간 기본값, 기록 writer/용량/retention 구현, 실제 이동/전투 입력과 전체 런 통과 선언. 새 이동 executor의 행동 직전 검사는 후속 통합의 소비자 의무다.

## Acceptance Contract

| ID | 결과 |
| --- | --- |
| OBS-01 | runtime 내부 freshness/대기/종료는 주입 가능한 단조 시계만 사용; 캡처 wall-clock은 기록용으로 분리 |
| OBS-02 | 명시적 invalidate와 창 영역 변경은 세대를 증가시키고 기존 frame/vision/decision·북쪽 재사용 캐시를 무효화 |
| OBS-03 | 이전 세대·미래/역순 source·같은 channel의 역순 결과는 최신 상태를 덮어쓰지 못함 |
| OBS-04 | 관찰 없음 결과는 이전 channel 결과를 계속 정상으로 남기지 않음; 최신 frame/최근 5개 상한 유지 |
| OBS-05 | 입력 없는 재생/가짜 capture에서 무효화 중 캡처·느린 인식·판단 publication을 검증하고 기존 계약 회귀 유지 |

## Trace

- Spec: [SPEC-0008](../spec/spec-0008-runtime-observation-provenance.md)
- Run: [RUN-20261009-01](../run/run-20261009-01-runtime-observation-provenance.md)
- Evaluation: [Contract](../evaluation/eval-0017-contract-runtime-observation-provenance.md), [Functional](../evaluation/eval-0018-functional-runtime-observation-provenance.md). 입력 없는 계약 경계 통과; 실게임 통합과 전체 PRD 수용은 별도다.
- Regression: 공통 필드·버프·생존 계약과 기존 북쪽 시간 비교·캐시 소비.
