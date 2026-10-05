# FEAT-0006: 필드 버프 확인과 벨트 관찰 종료

## Metadata

- ID: `feat-0006`; Status: `passed`; Type: `product`
- Surface / Profile: Python 공통 버프 트랜잭션·시각 자료 / `backend-product`
- Required Evaluators: Contract, Functional
- Parent PRD: [PRD-0004](../prd/prd-0004-summoner-survival-and-buffs.md)
- Created / Updated: `2026-10-05`
- Approval: 사용자가 다음 버프/생존 개발을 진행하라고 지시하고 아케인에서 직접 버프 테스트하도록 명시했다. 벨트는 관찰 뒤 닫고 이동한다. 전체 런의 확정 순서는 상위 경로/전투 PRD에 반영하며 이번 실행은 중앙의 버프 확인까지다.

## Goal And Scope

안전한 아케인 중앙에서 최신 사용자 순서 `w → a → a → s → F1 → d → w`를 한 단계씩 대조하고 실제 효과·최대값 변화·전투 상태 복귀를 확인한다. 게임 입력 전송과 확인된 버프 상태를 구분하는 공통 트랜잭션을 제공한다. 잔량 확인을 위해 벨트를 펼치면 완료 뒤 닫힘을 확인해야 일반 이동을 허용한다.

포함: 실제 감독 필드 자료, 재사용할 공통 시각 증거, 캐릭터/방별 단계·확인 상태와 안전/중단/시각 계약, 효과 확인 뒤 FEAT-0003 버프 시각에 연결, 벨트 관찰 종료 조건, 회귀와 실제 화면 대조. 입력은 감독 computer-use로 수행하고 제품 자동 실행기에 무조건 연결하지 않는다.

제외: 자동 주변 안전 판독을 확보하지 않은 무감독 버프, 필드 전투/포션 자동 사용/네 방향 완주, 다른 캐릭터 효과 추측, 기존 자산 이동/이름 변경. 미확인 다른 버프의 지속시간을 150초로 대신하지 않는다.

## Acceptance Contract

| ID | 결과 |
| --- | --- |
| BF-01 | 개인 순서를 그대로 사용하며 중복 a를 유지; 방/캐릭터/단계·증거 시각을 분리 |
| BF-02 | town/미확인/적 있음/오래된 화면/중단은 일반 버프 입력을 허가하지 않음 |
| BF-03 | 키 전송만으로 효과를 확정하지 않고 실제 버프 전후 현재/최대값과 시각 자료·복귀를 대조 |
| BF-04 | 확인 시각을 늦춰 버프 수명을 늘리지 않음; 안전한 완료 뒤 정책 타이머에 연결 |
| BF-05 | 벨트 펼침 관찰 뒤 닫힘 확인 전 이동 보류; 재관찰 실패/중단을 닫힘 성공으로 바꾸지 않음 |
| BF-06 | 실제 중앙 감독 확인·미확인/중단 합성과 기존 회귀를 구분하며 전체 생존/파밍 완료로 확대하지 않음 |

## Trace

- Spec: [SPEC-0005](../spec/spec-0005-field-buff-confirmation.md)
- Run: [RUN-20261005-04](../run/run-20261005-04-field-buff-confirmation.md)
- Contract: [EVAL-0011](../evaluation/eval-0011-contract-field-buff-confirmation.md)
- Functional: [EVAL-0012](../evaluation/eval-0012-functional-field-buff-confirmation.md)
