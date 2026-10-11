# FEAT-0015: 실제 main의 비상 포션 회복

- ID: `feat-0015`; Status: `approved`; Type: `product`
- Parent: [PRD-0004](../prd/prd-0004-summoner-survival-and-buffs.md)
- Approval: 사용자가 비율 기반 비상 회복과 구현·감독 검증을 승인했다. 새 운영 수치를 정하지 않고 확정된 생명력50%/마나10% 미만과 보라 포션 열을 사용한다.
- Profile: `backend-product`; Evaluators: Contract, Functional; Created: `2026-10-10`

main216은 대기 중 마나 비율 .0279에서 포션 입력 없이 안전 종료했다. 이동/전투/대기에서 현재 자원 부족을 발견하면 유지 키를 해제하고 현재 닫힌 벨트의 보라 포션을 확인한 열만1회 사용한다. 새 HUD 회복과 펼친 벨트의 실제 잔량·닫힘을 확인한 뒤 기존 동작으로 돌아간다. 키 전송을 사용/잔량 감소로 확정하지 않으며 실패·고갈은 기존 안전 종료로 보낸다.

다른 런/캐릭터도 같은 recovery consumer와 개인 정책을 사용할 수 있다. 자동 줍기/재보급/사망 복구와 전체 런 수용은 이번 경계에 포함하지 않는다. [SPEC-0014](../spec/spec-0014-live-emergency-recovery.md)와 통합 run Attempt10이 결과를 소유한다.
