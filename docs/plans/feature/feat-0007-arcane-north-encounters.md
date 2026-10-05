# FEAT-0007: 아케인 북쪽 지형·몬스터 조우 관찰

## Metadata

- ID: `feat-0007`; Status: `approved`; Type: `product`
- Parent PRD: [PRD-0006](../prd/prd-0006-summoner-combat-and-loot.md); 지형/이동 owner: [PRD-0005](../prd/prd-0005-arcane-navigation-and-recovery.md)
- Profile: `backend-product`; Required Evaluators: Contract, Functional
- Created / Updated: `2026-10-05`
- Approval: 사용자가 실제 아케인에서 버프 후 북쪽으로 이동해 몬스터를 조우하고, 회색 땅/우주 배경·맵 유형·접근 반경·생존/사망·지역별 자산 수집을 진행하도록 명시했다.

## Scope And Acceptance

한 방의 감독 북쪽 이동/조우/필요한 F4 노바 전후 자료와 입력 없는 공통 지형/몬스터 관찰을 구현한다. 필요한 공통 자료를 지역/배치 owner에 남기며 개인 이름/설정과 구분한다. 기존 자산은 외형/형식 적합성을 확인한 것만 재사용하고 무조건 일괄 변환/삭제하지 않는다.

| ID | 결과 |
| --- | --- |
| EN-01 | 실제 회색 발판과 우주 배경을 대조하고 바닥 색 후보를 도달 가능/전체 지형 검증으로 과장하지 않음 |
| EN-02 | 몬스터 후보의 종류/위치/점수와 지상 기준점·현재 프레임 맥락을 유지; 미확인 종류/위치는 미확인 |
| EN-03 | 개인 전투 반경을 명시적으로 소비하며 화면 크기 정규화·진입/이탈 경계를 분리; 임의 기본 공격 반경 없음 |
| EN-04 | 살아 있음/죽음/미확인을 구분하고 화면 이탈/미검출/노바 효과만으로 사망을 선언하지 않음 |
| EN-05 | 실제 북쪽 조우·감독 입력 결과와 합성 추적/거리/가림 검증을 분리; 자료 누락/실패도 기록 |

무감독 이동/전투/포션 실행기 연결, 네 방향 완주, 자동 보스 처치·획득 확인, 다른 맵 유형의 정확도는 포함하지 않는다. 실제 감독자는 최신 HUD/벨트/버프·적 접근을 확인하고 필요시 회복 또는 방 종료한다. 긴 입력 홀드나 필드에서 코드 작성 대기를 하지 않는다.

## Trace

- Spec: [SPEC-0006](../spec/spec-0006-arcane-north-encounters.md)
- Run: [RUN-20261005-05](../run/run-20261005-05-arcane-north-encounters.md)

## Current Result

공통 코드/실제 지역 자산·89개 회귀는 구현/통과했다. [Contract](../evaluation/eval-0013-contract-encounter-observation.md)는 PASS지만 [Functional](../evaluation/eval-0014-functional-encounter-observation.md)은 실제 감독140초 갱신 조건 실패로 FAIL이다. run은 `returned-to-spec`, feature acceptance는 보류다. 다음 live 확대는 실시간 생존/버프 감시·미확인 관찰 선점·포션 보충/닫힘과 실행기 연결 경계를 구체화한 뒤 진행한다.
