# EVAL-0013: 공통 조우·지형·반경·생존 계약

## Metadata

- ID: `eval-0013`; Status: `complete`; Evaluator Type: `contract`; Result: `PASS`
- Run ID: `run-20261005-05`; Attempt: `1`
- Feature: [FEAT-0007](../feature/feat-0007-arcane-north-encounters.md)
- Spec: [SPEC-0006](../spec/spec-0006-arcane-north-encounters.md)
- Execution Profile: `backend-product`
- Evidence Coverage: `complete` — 승인된 입력 없는 계약; 자동 생산자/실행기 제외
- Created: `2026-10-05`

## Checks And Evidence

primary가 [공통 계약](../../../diablo2/common/encounters.py), [지역 관찰기](../../../diablo2/common/encounter_vision.py), [manifest](../../../assets/regions/arcane-sanctuary/remastered-ko-1922x1140/README.md)와 소비 [검증](../../../tests/test_encounters.py)을 검토했다. 새17개가 통과했다. native crop→원본 박스/점수/지상점, 종류/단계/출처, PNG 바이트/소유 경로/크기, 현재 프레임·시각·캐릭터/방/지역/이동 세대가 보존된다. 입력/런 실행기를 import하지 않는다.

named verifier의 고정 packet 결과: 전체89/89, 변경 Python3개 compile·Black3개, PNG16개 geometry/출처와 문서7개 링크/Unicode 모두 종료0. 기존 생존/시전/HUD/버프/마을 consumer 회귀가 통과했다. 최종 semantic acceptance는 primary가 실제 운영 결과와 별도로 검토했다.

개인 반경은 필수 `CombatRadius`이며 enter/leave를 분리한다. 잘못된 수치/미확인 지상점·신단/사체/그룹·오래된/다른 프레임은 거리 확정을 거부한다. 이전 near 유지에는 같은 target_id와 fresh context가 필요하여 다른 몹/새 방/텔레포트에 상태를 넘기지 않는다. 현재 숫자를 Flash의 운영 기본값으로 설정하지 않았다.

생존 상태는 current positive alive, linked alive→dead와 독립 current dead, 그 외 unknown이다. 템플릿 후보/미검출/화면 이탈/노바 효과/단일 corpse 관찰을 dead로 바꾸지 않는다. 타입/연결/독립성은 외부 생산자가 책임지는 명시적 계약이며 이 단계는 자동 개체 추적을 주장하지 않는다. 실제 사체 무더기에는 개체 수/지상점을 지정하지 않았다.

지형 근거는 색 후보다. 팔레트 중복 제외·명시적 floor/void gate와 fresh patch 범위를 검사하며 landing/충돌/맵 연결을 주장하지 않는다. 기존 팔레트/공통 계산을 재사용하고 신단 음성 자료를 몬스터와 분리했다. 신규 공유 자료에는 개인 이름/방 식별/바인딩/반경이 없고 임시 원본 경로에 의존하지 않는다.

## Evidence Gaps And Route

자동 ground/identity/life/safety 생산자·새 자세 정확도·실제 반경 공격·실시간 통합은 이 입력 없는 계약에 비차단이나 live 확대에 필수다. 실제 감독 버프 갱신 실패는 [Functional report](eval-0014-functional-encounter-observation.md)가 소유하며 이 계약 PASS를 실제 운영 PASS로 확대하지 않는다. 차단 contract finding 없음. Route: `pass`.
