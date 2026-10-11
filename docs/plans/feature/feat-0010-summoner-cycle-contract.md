# FEAT-0010: Summoner 한 방 흐름과 결과 확인 계약

- ID: `feat-0010`; Status: `passed`; Type: `foundation`
- Parent: [PRD-0006](../prd/prd-0006-summoner-combat-and-loot.md), [PRD-0003](../prd/prd-0003-summoner-room-lifecycle.md)
- Profile: `foundation-contract`; Evaluators: Contract, Functional
- Created / Updated: `2026-10-09`
- Approval: 사용자의 전체 요구사항 확정·구현·감독 검증 지시. 이미 확정한 방 순서와 긍정 결과 확인을 입력 없는 제어 계약으로 구현한다.

## Boundary

캐릭터 선택→방 생성/Act 1→Arcane→라벨·버프·벨트 준비→2/4/8/10시 탐색→서모너 처치·전리품 확인→중앙 웨이포인트→Act 1 재정비→방 종료의 순서를 단일 제어기가 소유한다. 모든 명령은 한 번만 요청하고 새로운 화면의 의미 확인 전에는 다음 명령을 보내지 않는다. 실제 화면 의미 생산자와 입력 adapter는 후속 연결이며, 기존 GUI 기본 실행을 미검증 제어기로 바꾸지 않는다.

반복 수·화면 나이·단계/방/전체 시한은 호출자가 명시한다. 운영 기본값을 새로 정하지 않는다. 실패/중단/시한 초과는 유지 입력 해제와 사람 검토 요청으로 끝나며 다음 방을 만들지 않는다. 사용자는 실패 시 안전 종료 후 사람 확인, 사망 후 재정비 부재, 마을별 정비 분리를 확인했다. 실제 안전 종료 adapter는 후속 연결이며 이번 planner의 release를 종료 성공으로 보고하지 않는다. 전체 수용 기준은 감독 30방 중 27방 이상 정상 마감이다.

| ID | 결과 |
| --- | --- |
| CYCLE-01 | 긍정 선택/Act 1/Arcane/준비/종료 확인 없이 단계 진행이나 중복 명령 없음 |
| CYCLE-02 | 네 날개는 north/east/south/west; 빈 날개 확인 후 중앙 귀환, 보스 발견 뒤 처치·전리품 확인 후 남은 날개 생략 |
| CYCLE-03 | 단상 도착/미검출/클릭을 보스 없음·사망·획득으로 바꾸지 않고 명시적 의미 결과만 소비 |
| CYCLE-04 | 정상 결과는 Act 1 재정비와 선택 화면 복귀 뒤 정확히 한 번; 드롭 수와 획득 수 분리 |
| CYCLE-05 | 오래된/이전 명령 결과는 진행시키지 않음; 다른 방·캐릭터/포커스 상실·중단·단계/방/전체 시한은 입력 해제, 실패 후 자동 반복 없음 |

- Spec: [SPEC-0009](../spec/spec-0009-summoner-cycle-contract.md)
- Run: [RUN-20261009-02](../run/run-20261009-02-summoner-cycle-contract.md)
- Evaluation: [Contract](../evaluation/eval-0019-contract-summoner-cycle.md), [Functional](../evaluation/eval-0020-functional-summoner-cycle.md): 입력 없는 방 제어 계약 통과. 실제 adapter/전체 런 검증은 별도다.
