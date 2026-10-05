# FEAT-0005: 생존 HUD·벨트 관찰 전용 판독

## Metadata

- ID: `feat-0005`; Status: `passed` — 제한한 배치의 관찰 전용 기능; 사람 수용·후속 자동 제어는 run에서 구분
- Type: `product`; Surface: Python 화면 판독·CLI
- Execution Profile: `backend-product`; Required Evaluators: Contract, Functional
- Parent PRD: [PRD-0004](../prd/prd-0004-summoner-survival-and-buffs.md)
- Created / Updated: `2026-10-05`
- Approval: 사용자가 화면 테스트/캡처와 계속 구현을 요청하고 필요한 조작을 직접 수행하도록 승인했다. 필드에서는 새 방으로 마을에 진입하도록 지시했다. 이번 기능은 안전한 마을의 자료 준비와 입력 없는 판독/판단 표시까지 고정한다.

## Goal And Scope

화면의 생명력·마나 숫자와 펼친 벨트 슬롯을 실제 이미지에서 읽어 관찰 전용 결과로 표시한다. 기존 상태 계약에 값을 가정해서 넣지 않는다. shared HUD/숫자/아이콘 자료는 개인 이름이나 키 배치와 분리한다.

포함: 현재 확인한 한국어 리마스터 창 배치의 공통 시각 자료, 엄격한 숫자 판독과 미확인 처리, 펼친 벨트의 슬롯 종류/열별 잔량, bounded 단발/짧은 CLI 캡처와 offline 판독, FEAT-0003 정책의 입력 없는 요청 표시, 실제 마을 화면 대조와 회귀 검증.

제외: 필드 이동/전투·포션 자동 사용/고갈 종료 배우·재버프·버프 효과/주변 적 없음/마을 자동 판정·모든 해상도/언어·실시간 control 통합·기존 방 자산 이동/파일명 정리. 수집한 화면 상태 밖의 신뢰도와 생존 성공을 주장하지 않는다.

## Acceptance Contract

| ID | 결과 |
| --- | --- |
| HUD-01 | 검토한 shared label/glyph/item 자료와 layout ID를 사용하며 개인 이름/FCR/포션 배치를 시각 규칙에 하드코딩하지 않음 |
| HUD-02 | 실제 마을의 생명력 985/985·마나 1075/1075를 화면에서 읽고 숫자 불명확/잘못된 범위·레이아웃/가림은 미확인; 최대값이 바뀌면 같은 프레임의 현재값/최대값으로 임계 비율을 계산 |
| HUD-03 | 펼친 벨트의 16슬롯 종류를 읽어 포션 12개/스크롤 4개와 비어 있는 슬롯을 구분; 닫힌/가린/미지원 슬롯을 0으로 만들지 않음 |
| HUD-04 | 판독된 전체 열만 개인 정책에 대조하며 오류/불일치로 상태를 확정하지 않음. 버프 확인·field/clear 신호를 생성하지 않음 |
| HUD-05 | 캡처/판독 시각·layout·자료 revision·판독 이유를 가진 관찰 전용 결과를 제공; CLI는 게임 입력을 보내지 않고 제한된 프레임만 처리 |
| HUD-06 | 실제 마을 자료/독립 후속 프레임과 closed/occluded/empty/boundary 합성 자료를 구분해서 검증하고 기존 47개 회귀 유지 |

## Dependencies And Trace

기존 ScreenCapture/OpenCV·NumPy, [FEAT-0003](feat-0003-character-survival-state.md)의 개인 정책·상태 계약을 사용한다. PRD-0008은 후속 실제 control 통합의 조건이며 단발 관찰 CLI의 blocker가 아니다. 현 배치 밖은 보류한다.

- Spec: [SPEC-0004](../spec/spec-0004-survival-screen-observation.md)
- Run: [RUN-20261005-03](../run/run-20261005-03-survival-screen-observation.md)
