# EVAL-0019: Summoner 방 제어 계약

- ID: `eval-0019`; Status: `complete`; Evaluator Type: `contract`; Result: `PASS`
- Feature: [FEAT-0010](../feature/feat-0010-summoner-cycle-contract.md); Spec: [SPEC-0009](../spec/spec-0009-summoner-cycle-contract.md)
- Run: [RUN-20261009-02](../run/run-20261009-02-summoner-cycle-contract.md); Attempt: `1`
- Profile: `foundation-contract`; Evidence Coverage: `complete` — 순수 planner, 실제 adapter 제외
- Created: `2026-10-09`

primary가 전체 단계/실패 순서, 명령과 결과의 관계, 시한·맥락 검사와 테스트를 검토했다. 시작 선택과 생성 후 Act 1 도착을 구분하며 결과는 현재 명령 이후 frame·monotonic 시각·현재 이상 세대에 연결한다. 입력 전송은 긍정 결과가 아니고, 일반 끝 템플릿·미검출·이름표 소실·클릭은 각각 보스 부재/사망/획득의 생산자가 될 수 없다고 spec에 명시했다.

모든 날개는 중앙 귀환 뒤 다음 날개로 연결한다. 발견한 보스의 사망/전리품 확인 뒤 남은 탐색을 생략한다. 정상 결과는 Act 1 정비와 선택 복귀 뒤 한 번만 기록한다. 드롭/획득과 실제 방 진입 여부를 분리하고 미회수/모든 날개 부재/명시적 실패/시한/중단은 검토 요청으로 종료한다. 실패 후 자동 생성은 없다.

시간 상한·유한 room ID 목록은 호출자 소유이며 중복/빈 목록을 거부한다. 내부 결과 보유는 명시한 방 개수 이하다. wait는 새 입력 허가가 아니며 실제 field executor의 최신성·유지 시한 검사를 대신하지 않는다. release는 adapter가 실행할 요청이고 실제 OS 해제·안전 종료 성공이 아니다. 이 기능을 기존 GUI/전체 런에 미검증 상태로 연결하지 않았다.

155개 전체 회귀와 2개 Black/compile·diff check 통과. 실제 시각 생산자/안전 종료·귀환·정비/GUI 통합은 후속이며 본 입력 없는 계약에 차단 finding 없음. Route: `pass`.
