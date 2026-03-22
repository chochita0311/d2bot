# `north_go` 현재 동작 설명

이 문서는 [north_go.py](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py)의 현재 동작을 기준으로 정리한 문서입니다.

이 라우트에서 쓰는 시계 방향 표기는 아래와 같습니다.

- `north` = `2 o'clock`
- `west` = `10 o'clock`
- `east` = `4 o'clock`

`north_go`는 한 번 목표 좌표를 정하고 끝나는 함수가 아니라, 실시간으로 화면을 계속 읽으면서 방향 family와 candidate를 다시 고르고, 커서를 조정하고, Arcane terminal이 검출되면 멈추는 제어 루프입니다.

## 1. 시작 단계

진입점은 [run_arcane_north_go()](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py)입니다.

시작할 때 하는 일은 아래와 같습니다.

1. 캐릭터에 이동 스킬이 설정되어 있는지 확인합니다.
2. [prepare_arcane_hub_start()](/D:/python/d2bot/diablo2/runs/summoner/routes/common/arcane_common.py)로 Arcane hub 기준점을 맞춥니다.
3. hub focus ratio를 읽어서 이번 루프의 zero point로 저장합니다.
4. 이전 방향, 이전 ratio, 현재 family 같은 제어 상태를 초기화합니다.
5. `RealtimeVisionRuntime`를 시작하고 fast vision, slow vision, decision 콜백을 연결합니다.

즉 시작 단계의 목적은 "북쪽 경로 탐색을 하기 전에 hub 기준 좌표를 먼저 안정적으로 맞추는 것"입니다.

## 2. 런타임 구조

`north_go`는 `RealtimeVisionRuntime` 위에서 돌아갑니다.

핵심 콜백은 아래 3개입니다.

- `_fast_vision()`
- `_slow_vision()`
- `_decision()`

capture 스레드는 최신 프레임을 계속 공급하고, fast/slow/decision이 그 프레임을 서로 다른 용도로 사용합니다.

## 3. Fast Vision 단계

`_fast_vision()`은 방향 선택을 위한 고주파 계산 단계입니다.

여기서 계산하는 값은 아래와 같습니다.

- `fast_maps`
- 모든 direction candidate의 vote
- family gate 신호
- frame progress change
- progress trend

### 3-1. fast_maps

[_build_arcane_fast_maps()](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py)는 현재 프레임을 축소해서 아래 두 mask를 만듭니다.

- `floor_mask`
- `star_mask`

이 mask들은 이후 direction scoring과 family gate scoring에서 공통으로 사용됩니다.

### 3-2. direction candidate vote

[_score_arcane_direction_candidates()](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py)는 모든 candidate를 점수화합니다.

candidate family 구성은 아래와 같습니다.

- north: `north_primary`, `north_soft`, `north_sharp`
- west: `west_primary`, `west_soft`, `west_sharp`
- east: `east_primary`, `east_soft`, `east_sharp`

각 candidate 점수는 아래 구조입니다.

`score = openness + bias + continuity_bonus`

의미는 아래와 같습니다.

- `openness`
  - 그 방향 경로가 floor처럼 열려 보이는 정도
- `bias`
  - candidate 자체의 작은 기본 선호도
- `continuity_bonus`
  - 직전 커서 ratio와 가까우면 조금 더 점수를 주는 보정

## 4. Family Gate

지금은 north만 gate가 있는 구조가 아니라, north / west / east 모두 gate 신호를 계산합니다.

관련 함수:

- [_score_arcane_north_open_signal()](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py)
- [_score_arcane_west_open_signal()](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py)
- [_score_arcane_east_open_signal()](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py)
- [_score_arcane_family_signals()](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py)

동작 방식:

- north gate는 north probe points를 사용합니다.
- west gate는 west probe points를 사용합니다.
- east gate는 east probe points를 사용합니다.
- 각 probe 주변을 원형 영역으로 샘플링해서 floor-like 비율을 gate 신호로 사용합니다.

즉 지금 구조는 "candidate vote"와 "family gate"가 분리되어 있습니다.

## 5. 방향 선택 규칙

최종 선택 함수는 [_choose_arcane_direction()](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py)입니다.

현재 선택 순서는 아래와 같습니다.

1. north gate가 충분히 열려 있으면 north family에서 가장 좋은 candidate를 즉시 선택합니다.
2. north gate가 닫혀 있으면 west gate와 east gate를 먼저 비교합니다.
3. side family가 정해지면 그 family 내부에서 vote가 가장 좋은 candidate를 사용합니다.
4. family hysteresis와 candidate hysteresis를 적용해서 너무 쉽게 흔들리지 않게 합니다.

side family 비교는 [_choose_arcane_side_family_vote()](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py)에서 처리합니다.

현재 중요한 hysteresis 상수는 아래와 같습니다.

- `ARCANE_SIDE_GATE_KEEP_MARGIN = 0.22`
- `ARCANE_VOTE_KEEP_MARGIN = 0.045`

의미:

- west/east family를 바꾸려면 gate 차이가 충분히 커야 합니다.
- 차이가 작으면 기존 side family를 유지합니다.
- family가 정해진 뒤에는 그 family 내부에서 candidate vote를 사용합니다.

## 6. Slow Vision 단계

`_slow_vision()`은 상대적으로 무거운 판단을 담당합니다.

여기서 계산하는 값은 아래와 같습니다.

- `terminal`
- `loot_label`
- `monster_hit`
- `hover_blocker_kind`

slow payload는 fresh할 때만 decision 단계에서 신뢰합니다.

현재 관련 상수:

- `ARCANE_SLOW_STALE_LIMIT_MS = 1800`

즉 slow vision 결과는 최대 1.8초까지 유효한 판단으로 봅니다.

## 7. Terminal 감지

terminal 감지는 이제 `north_go.py` 내부 전용 로직이 아니라 Arcane 공통 helper로 옮겨져 있습니다.

위치는 [arcane_common.py](/D:/python/d2bot/diablo2/runs/summoner/routes/common/arcane_common.py) 안의
[detect_arcane_terminal()](/D:/python/d2bot/diablo2/runs/summoner/routes/common/arcane_common.py#L201) 입니다.

현재 terminal 감지 기준:

- `assets/waypoint/act2/goal_center.png`
- `ARCANE_GOAL_CENTER_THRESHOLD`

중요한 점:

- 예전처럼 Summoner 전용 terminal template을 쓰지 않습니다.
- 예전처럼 Summoner clue를 여기서 같이 보지 않습니다.
- Summoner 발견 여부는 별도 로직에서 처리할 예정입니다.

즉 현재 terminal은 "Arcane route 끝 공통 floor goal center가 보이느냐"만 판단합니다.

## 8. 멈춤 조건

`north_go`는 [_decision()](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py) 안에서 아래 조건이 만족되면 멈춥니다.

1. slow payload가 존재하고
2. slow payload가 fresh하고
3. `terminal`이 `None`이 아니면

그때 하는 일:

1. Arcane goal center를 감지했다는 로그를 남깁니다.
2. `session.request_stop()`을 호출합니다.
3. decision callback은 `terminal` 상태를 반환합니다.

## 9. Steering 단계

최종 candidate가 정해진 뒤에는 [_steer_arcane_movement()](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py)가 실제 커서 위치를 결정합니다.

현재 하는 일은 아래와 같습니다.

1. 필요하면 floor-guided ratio 보정을 합니다.
2. family가 west/east이면 안쪽으로 조금 당겨서 클릭합니다.
3. 최종 ratio로 커서를 이동합니다.
4. side family에서 north로 다시 열릴 때는 짧은 fast reacquire를 사용합니다.
5. chest/teleporter hover가 걸리면 nudge로 살짝 피합니다.

## 10. stale-fast hold

fast vision이 너무 오래되면 새 방향 결정을 하지 않습니다.

대신 아래 값을 재사용합니다.

- `last_direction_key`
- `last_direction_ratio`
- `route_family`

즉 "지금 fast frame이 믿기 어렵기 때문에 직전에 가던 방향을 잠깐 유지한다"는 fallback 모드입니다.

## 11. 현재 north_go 요약

지금 `north_go`는 아래 흐름으로 동작합니다.

1. Arcane hub 기준점을 맞춥니다.
2. fast vision으로 모든 candidate vote와 family gate를 계속 계산합니다.
3. slow vision으로 terminal / monster / loot / hover를 계속 계산합니다.
4. north gate가 열려 있으면 north family를 우선합니다.
5. north gate가 닫히면 west vs east를 gate로 먼저 비교합니다.
6. 선택된 family 내부에서 가장 좋은 candidate를 사용합니다.
7. hysteresis로 family/candidate 흔들림을 줄입니다.
8. steering 단계에서 floor-guided 보정, inward scaling, hover 회피를 적용합니다.
9. shared Arcane goal center terminal을 감지하면 멈춥니다.

## 12. 참고 메모

- 이 문서는 현재 generic `goal_center` terminal 구조를 기준으로 작성되었습니다.
- 예전 Summoner 전용 terminal 감지는 더 이상 `north_go`의 일부가 아닙니다.
- 이후 `south_go`, `east_go`, `west_go`도 같은 terminal helper를 재사용할 수 있습니다.
