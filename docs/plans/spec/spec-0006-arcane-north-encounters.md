# SPEC-0006: 감독 조우 자료와 공통 위치/상태 계약

## Metadata

- ID: `spec-0006`; Status: `ready`; Run ID: `run-20261005-05`; Attempt: `1`
- Feature: [FEAT-0007](../feature/feat-0007-arcane-north-encounters.md)
- Profile: `backend-product`; Required Evaluators: Contract, Functional
- Created / Updated: `2026-10-05`

## Contract

최신 화면을 보고 한 번의 입력 뒤 재관찰한다. 중앙에서 승인된 순서로 버프하고 벨트를 닫는다. 북쪽 통로의 관찰한 회색 발판만 감독 목표로 사용하며 우주 배경으로 입력하지 않는다. 적 접근에는 짧은 F4 노바와 최신 생명력/마나 확인을 사용한다. 위험·미관찰·버프 갱신 필요·포션 고갈이면 자료 수집보다 생존/귀환/종료가 우선이다.

제품 코드는 입력 없는 관찰/상태 계약이다. 지형 색 마스크는 화면상 바닥 후보이지 충돌/텔레포트 성공 증명이 아니다. 현재 관찰한 배치의 지역별 자료와 지상 기준점/박스/정답 출처를 함께 저장한다. 몬스터 템플릿은 살아 있는 자료와 사망 자료를 구별하고 후보 점수/위치를 보존한다. 단순 미검출은 사망 증거로 쓰지 않는다.

공통 거리 계산은 현재 프레임의 캐릭터 발·몬스터 지상 기준점과 명시된 화면 크기를 사용한다. 개인 반경 진입/이탈 값은 호출자가 명시하고 모든 값은 유효한 정규화 범위여야 한다. 시각/방/캐릭터·이동 세대가 달라진 옛 자료를 새 입력 근거로 쓰지 않는다. 현재 장면의 반경 비교는 실험값이며 개인 설정의 실게임 기본값으로 채택하지 않는다.

추적은 명시적 살아 있음/죽음 증거, 가림/미검출, 최신 맥락을 분리한다. 사망에는 같은 대상의 변화/독립 사후 관찰이 필요하다. 재생 자료의 사람 정답과 검증 결과를 저장하고 감독 조우를 자율 경로/무감독 안전 성공으로 계수하지 않는다.

## Validation And Limits

실제 native 창의 중앙→북쪽 발판 이동·적 조우·공격 전후를 대조한다. 한 맵 유형/한 방 자료를 모든 레이아웃 정확도로 확대하지 않는다. 합성은 반경 경계/가림/프레임 이동·잘못된 방/캐릭터·사망 미확인·형식 오류를 확인한다. 기존 생존/버프/시전/마을 회귀를 유지한다. 세션 종료 전 필요한 이름 없는 자료만 assets에 선별하고 bounded 원본/진단을 정리한다.

## Implementation Contract

- [encounters.py](../../../diablo2/common/encounters.py)의 `SceneContext`, `VisualCandidate`, `CombatRadius`, `proximity`, `LifeWitness`, `life_state`가 공통 계약이다. 거리 정규화는 `hypot(dx/native_width, dy/native_height)`다. near 유지에는 명시적으로 연결한 동일 대상과 같은 이동 세대의 최신 이전 관찰이 필요하다.
- [encounter_vision.py](../../../diablo2/common/encounter_vision.py)의 `RegionObserver`는 지역 manifest를 소비하고 원본 화면 박스/점수/지상점·외형 후보를 반환한다. PNG 확장자/실제 바이트·소유 경로·native geometry를 검사하며 입력 실행기/런 패키지를 import하지 않는다. 바닥/우주 팔레트 중복 픽셀은 양쪽에서 제외한다.
- 사망 확인에는 같은 종류/연결된 대상/방/캐릭터/세대의 alive, linked dead, 더 최신인 독립 dead 관찰이 필요하다. 세 관찰의 시각/순번이 증가하고 모두 명시된 신선도 범위여야 한다. `template-candidate`는 양성 생존 증거가 아니다. 이 경계는 자동 개체 연관 생산자를 구현하지 않는다.
- [공통 지역 자산](../../../assets/regions/arcane-sanctuary/remastered-ko-1922x1140/README.md)의 alive/corpse/shrine 분리와 JPEG 출처·native crop·한계가 재생 소비자와 함께 남는다. 실제 반경은 진단 비교이며 개인 운영값은 설정하지 않는다.
