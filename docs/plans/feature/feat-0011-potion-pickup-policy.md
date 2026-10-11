# FEAT-0011: 캐릭터별 포션 회수 종류와 TP 목표

- ID: `feat-0011`; Status: `passed`; Type: `foundation`
- Parent: [PRD-0007](../prd/prd-0007-shared-loot-and-town-maintenance.md), [PRD-0004](../prd/prd-0004-summoner-survival-and-buffs.md)
- Profile: `foundation-contract`; Evaluators: Contract, Functional; Created / Updated: `2026-10-09`
- Approval: 사용자가 기본 대량 활력, 현재 잔량6개 이하에서만 작은 활력 허용,4열TP4장을 명시했다.

공통 자원 상태에 캐릭터별 회수 종류를 추가한다. Flash의 현재 확인된 잔량이7개 이상이면 작은 활력을 허용하지 않으며6개 이하일 때만 허용한다. 가득 참/미확인/오래된 벨트에서는 회수 종류를 허가하지 않고0개 강제 종료가 우선이다. TP 목표를 관찰 baseline이 아닌 승인한4장으로 설정한다. 다른 캐릭터의 종류/임계값은 선택한 프로필에서만 읽는다.

포함: 선택적 policy schema·입력 없는 회수 종류 판단·TP 결손과 경계 회귀. 제외: 지상 라벨 식별·작은 포션 외형 판독·실제 클릭/획득·마을 구매/정비, 감정/귀환 책·골드 기준. 허용 종류는 실제 입력 허가가 아니다.

- Spec: [SPEC-0010](../spec/spec-0010-potion-pickup-policy.md)
- Run: [RUN-20261009-04](../run/run-20261009-04-potion-pickup-policy.md)
- Evaluation: [Contract](../evaluation/eval-0021-contract-potion-pickup-policy.md), [Functional](../evaluation/eval-0022-functional-potion-pickup-policy.md): 입력 없는 정책 경계 통과. 실제 종류 인식/획득은 별도다.

| ID | 결과 |
| --- | --- |
| PICK-01 | 개인 preferred/fallback/현재잔량 임계값을 검증·로딩하고 다른 프로필에 Flash 규칙을 강제하지 않음 |
| PICK-02 | 7/6/0/12개와6→7/7→6에서 종류를 다시 판단; stale/future/unknown 상태는 허용 없음 |
| PICK-03 | Flash TP 목표4장과 실제 확인 잔량의 차이를 결손으로 보고; 다른 null 목표의 첫 관찰 baseline 계약 유지 |
