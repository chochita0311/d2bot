# EVAL-0009: 생존 화면 관찰 계약

## Metadata

- ID: `eval-0009`; Status: `complete`; Evaluator Type: `contract`; Result: `PASS`
- Run ID: `run-20261005-03`; Attempt: `1`
- Feature: [FEAT-0005](../feature/feat-0005-survival-screen-observation.md)
- Spec: [SPEC-0004](../spec/spec-0004-survival-screen-observation.md)
- Execution Profile: `backend-product`
- Evidence Coverage: `complete` — 승인한 배치의 관찰 전용 계약; 일반 필드 생존 readiness는 포함하지 않음
- Created: `2026-10-05`

## Checks And Evidence

primary가 요구·자료 소유권·구현·최종 의미를 검토했다. named evidence scout는 기존 자료/캡처 API만 조사하고 bounded verifier는 고정 검증 출력만 확인했다. 설계·진행 허가·사람 수용을 worker 결과로 대신하지 않는다.

- HUD-01: [shared manifest와 자료](../../../assets/ui/survival/README.md)는 이름 없는 HUD/숫자/아이템 외형을 소유한다. [공통 판독기](../../../diablo2/common/survival_vision.py)는 layout/revision과 엄격한 모양·경로·유한 matching 설정을 검증하며 개인 이름/키/임계값을 시각 규칙에 넣지 않는다. 기존 private 자산을 변경하지 않았다.
- HUD-02: 두 label과 전체 native 크기를 확인하고 current/max를 같은 프레임에서 읽는다. 숫자 범위·모호함·가림은 null, 이전 최대값/비율을 재사용하지 않는다. 버프에 따른 최대값 변경과 정확히 50%/10% 경계는 합성 자료로 검증했다.
- HUD-03/04: 펼친 16슬롯의 유일한 위치/종류와 개인 열별 capacity를 확인한 결과만 상태 계약에 전달한다. 실제 12/11개·닫힘과 합성 가림/잘못된 열/중복 슬롯을 구분한다. 미관찰을 0으로 만들지 않으며 field/clear·버프 확인을 생성하지 않는다.
- HUD-05/06: [CLI](../../../diablo2/tools/survival_observe.py)는 live/image provenance, 캡처/처리 경계, revision/이유와 제한된 1–5프레임을 제공한다. 최소화/숨김/누락 창은 거부하고 캡처를 닫는다. 입력 actor는 없고 정책 진단의 프로필/방/age limit은 명시적으로 요구한다. 각 프레임의 벨트·자원을 독립 판단한다.

## Evidence Gaps And Route

같은 마을에서 채집한 HUD crop와 후속 live를 검증했으며 독립 방/창/필드 정확도·실제 버프 전후·저체력·모든 슬롯 외형·자동 회복/종료는 미검증이다. 2/6은 캐릭터 패널의 글꼴 후보이므로 모든 HUD 숫자의 지원으로 확대하지 않는다. 호출 시각은 렌더러 시각/게임 입력 지연을 증명하지 않는다. 이 한계는 관찰 전용 기능에 비차단이며 실제 제어 계약에서 해소해야 한다.

차단 finding 없음. Route: `pass`; 후속 자동 입력이나 사람 수용을 승인하지 않는다.
