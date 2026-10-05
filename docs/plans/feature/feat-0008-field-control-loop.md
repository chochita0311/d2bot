# FEAT-0008: 필드 이동·전투 유지 입력과 생존 선점

## Metadata

- ID: `feat-0008`; Status: `passed`; Type: `product`
- Parent: [PRD-0006](../prd/prd-0006-summoner-combat-and-loot.md); 생존 owner: [PRD-0004](../prd/prd-0004-summoner-survival-and-buffs.md)
- Profile: `backend-product`; Evaluators: Contract, Functional
- Created / Updated: `2026-10-05`
- Approval: 사용자의 “끝까지 완료될 때까지 계속”과 F2 유지 이동→F4 유지 공격→정리/아이템 확인→F2 이동 재개 지시. 기존 입력 없는 관찰 경계를 실행 제어까지 확장한다. 별도 재승인을 요청하지 않는다.

## Goal And Acceptance

모든 런이 재사용하는 단일 입력 소유자와 최신 화면 기반 필드 제어기를 구현한다. 몬스터/지형 후보를 살아 있음/안전/사망 확정으로 승격하지 않는다. 현재 실제 화면 인식과 유지 입력 시험이 가능한 범위까지 연결하고, 불가능한 부분은 성공으로 보고하지 않는다.

| ID | 결과 |
| --- | --- |
| FC-01 | F2와 F4가 겹치지 않으며 최신 안전한 관찰에서 유지, 전환·일시정지·중단·예외·화면 지연에서 해제 |
| FC-02 | 공격은 시간 상한을 갖고 사망/전리품 확인 뒤에만 이동 재개; 미검출은 사망 아님 |
| FC-03 | 포션·확인된 고갈 종료·140초 버프 갱신 판단이 이동/공격보다 우선하고 화면 처리 중단에도 키를 해제 |
| FC-04 | 포션 입력은 확인된 소비와 분리하며 중복 전송 금지; 펼친/미확인 벨트에서는 이동/공격 금지 |
| FC-05 | 직업/FCR 시전 참고값과 관측 도착 시간을 소비하는 적응형 조준 간격; 미확인 도착은 무한 홀드 금지 |
| FC-06 | 실제 화면·주입 백엔드·재생 결과를 구분; 네 방향 완주/자동 처치/열쇠 획득을 입력 로그만으로 선언하지 않음 |

## Trace

- [SPEC-0007](../spec/spec-0007-field-control-loop.md)
- [RUN-20261005-06](../run/run-20261005-06-field-control-loop.md)
- [Contract](../evaluation/eval-0015-contract-field-control-loop.md), [Functional](../evaluation/eval-0016-functional-field-control-loop.md): 공통 계약/주입형 검증과 짧은 감독 유지/연속 입력 경계 통과. 자동 의미 생산자와 전체 런 수용은 포함하지 않는다.

전체 런의 목표/순서는 유지한다. 기존 공유 자산의 일괄 이동/이름 변경, 무검증 반경 기본값, 장시간 무감독 반복은 이 단계에서 도입하지 않는다.
