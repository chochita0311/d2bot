# EVAL-0017: 실시간 관찰 시각·세대 계약

- ID: `eval-0017`; Status: `complete`; Evaluator Type: `contract`; Result: `PASS`
- Run: [RUN-20261009-01](../run/run-20261009-01-runtime-observation-provenance.md); Attempt: `1`
- Feature: [FEAT-0009](../feature/feat-0009-runtime-observation-provenance.md); Spec: [SPEC-0008](../spec/spec-0008-runtime-observation-provenance.md)
- Profile: `foundation-contract`; Evidence Coverage: `complete` — 승인한 입력 없는 시계/세대 경계
- Created: `2026-10-09`

primary가 runtime, 북쪽 소비자, 테스트와 spec을 검토했다. capture 시작 직전 monotonic 시각에서 수명을 계산하며 wall-clock을 freshness 비교에 사용하지 않는다. capture/vision/decision의 source 세대·순번·시각을 보존하고, 같은 channel의 중복/역순 source와 미래 시각을 거부한다. None 결과는 이전 vision을 지우며 최근 frame은 5개까지만 보유한다.

invalidate와 창 영역 변경은 기존 채택 결과를 지우고 늦은 publication을 거부한다. 북쪽 캐시는 세대별로 분리하고 유지 입력을 해제한다. 행동 직전 세대를 재확인하지만 검사와 OS 입력의 원자성, 이미 실행된 callback 입력의 취소를 보증하지 않는다. 후속 executor의 이동/방/장비 변경 시 invalidate 호출과 현재 세대·나이 검사 의무를 spec에 남겼다.

전체 141개 unittest와 변경 Python 3개의 Black/compile, diff whitespace 검증이 통과했다. 새 테스트 12개는 느린 capture/worker/decision과 무효화의 순서 역전, 창 영역 변경, 시계 역행, backlog 상한을 포함한다. 세부 결과는 run과 [Functional](eval-0018-functional-runtime-observation-provenance.md)이 소유한다.

자동 안전·실제 도착/사망/획득, writer/retention, 실게임 전체 런은 이번 계약에 포함하지 않는다. 그 미완료를 통과로 바꾸는 finding은 없다. Route: `pass`.
