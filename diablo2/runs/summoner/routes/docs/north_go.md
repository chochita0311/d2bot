# `north_go` 현재 동작 설명

이 문서는 [north_go.py](../north_go.py)의 현재 동작을 기준으로 정리한 문서입니다.

이 라우트에서 쓰는 시계 방향 표기는 아래와 같습니다.

- `north` = `2 o'clock`
- `west` = `10 o'clock`
- `east` = `4 o'clock`

`north_go`는 한 번 목표 좌표를 정하고 끝나는 함수가 아니라, 실시간으로 화면을 계속 읽으면서 방향 family와 candidate를 다시 고르고, 커서를 조정하고, Arcane terminal이 검출되면 멈추는 제어 루프입니다.

## 1. 시작 단계

진입점은 [run_arcane_north_go()](../north_go.py)입니다.

시작할 때 하는 일은 아래와 같습니다.

1. 캐릭터에 이동 스킬이 설정되어 있는지 확인합니다.
2. [prepare_arcane_hub_start()](../common/arcane_common.py)로 Arcane hub 기준점을 맞춥니다.
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

[_build_arcane_fast_maps()](../north_go.py)는 현재 프레임을 축소해서 아래 두 mask를 만듭니다.

- `floor_mask`
- `star_mask`

이 mask들은 이후 direction scoring과 family gate scoring에서 공통으로 사용됩니다.

### 3-2. direction candidate vote

[_score_arcane_direction_candidates()](../north_go.py)는 모든 candidate를 점수화합니다.

candidate family 구성은 아래와 같습니다.

- north: `north_primary`, `north_soft`, `north_sharp`
- west: `west_primary`, `west_soft`, `west_sharp`, `west_lower_soft`
- east: `east_primary`, `east_soft`, `east_sharp`, `east_upper_sharp`

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

- [_score_arcane_north_open_signal()](../north_go.py)
- [_score_arcane_west_open_signal()](../north_go.py)
- [_score_arcane_east_open_signal()](../north_go.py)
- [_score_arcane_family_signals()](../north_go.py)

동작 방식:

- north gate는 north probe points를 사용합니다.
- west gate는 west probe points를 사용합니다.
- east gate는 east probe points를 사용합니다.
- 각 probe 주변을 원형 영역으로 샘플링해서 floor-like 비율을 gate 신호로 사용합니다.

즉 지금 구조는 "candidate vote"와 "family gate"가 분리되어 있습니다.

## 5. 방향 선택 규칙

후보 선택 함수는 [_choose_arcane_direction()](../north_go.py)이고, 실제 전환은 `_stabilize_arcane_turn_choice()`를 거칩니다.

현재 선택 순서는 아래와 같습니다.

1. north gate가 충분히 열려 있으면 north family에서 가장 좋은 candidate를 우선 제안합니다.
2. north gate가 닫혀 있으면 west gate와 east gate를 먼저 비교합니다.
3. side family가 정해지면 그 family 내부에서 vote가 가장 좋은 candidate를 사용합니다.
4. family hysteresis와 candidate hysteresis를 적용해서 너무 쉽게 흔들리지 않게 합니다.

side에서 north로 전환할 때는 `north_open >= 0.38`인 fresh 판단 2회를 확인합니다. 횟수가 부족하고 기존 side 후보를 유지할 수 있으면 전환을 보류합니다.

side family 비교는 [_choose_arcane_side_family_vote()](../north_go.py)에서 처리합니다.

현재 중요한 hysteresis 상수는 아래와 같습니다.

- `ARCANE_SIDE_GATE_KEEP_MARGIN = 0.9`
- `ARCANE_VOTE_KEEP_MARGIN = 0.035`

의미:

- west/east family를 바꾸려면 gate 차이가 충분히 커야 합니다.
- 차이가 작으면 기존 side family를 유지합니다.
- family가 정해진 뒤에는 그 family 내부에서 candidate vote를 사용합니다.

## 6. Slow Vision 단계

`_slow_vision()`은 상대적으로 무거운 판단을 담당합니다.

여기서 계산하는 값은 아래와 같습니다.

- `end`
- `loot_label`
- `monster_hit`
- `hover_blocker_kind`

slow payload는 fresh할 때만 decision 단계에서 신뢰합니다.

현재 관련 상수:

- `ARCANE_SLOW_STALE_LIMIT_MS = 1800`

즉 slow vision 결과는 최대 1.8초까지 유효한 판단으로 봅니다.

## 7. Terminal 감지

terminal 감지는 이제 `north_go.py` 내부 전용 로직이 아니라 Arcane 공통 helper로 옮겨져 있습니다.

위치는 [arcane_common.py](../common/arcane_common.py) 안의
[detect_arcane_end()](../common/arcane_common.py) 입니다.

현재 end 감지 기준:

- `assets/waypoint/act2/goal_center.png`
- `assets/waypoint/act2/goal_center_covered_gold.png`
- 위 goal center 계열에는 `ARCANE_GOAL_CENTER_THRESHOLD`를 적용합니다.
- 공용 chest/coffin end 후보 5개 중 3개 이상이 `ARCANE_END_CHEST_THRESHOLD`를 통과해도 end로 인정합니다.

중요한 점:

- 예전처럼 Summoner 전용 terminal template을 쓰지 않습니다.
- 예전처럼 Summoner clue를 여기서 같이 보지 않습니다.
- Summoner 발견 여부는 별도 로직에서 처리할 예정입니다.

즉 현재 end는 공용 goal center 또는 chest/coffin 특징으로 경로 끝을 판단하며, 보스 처치 완료를 의미하지 않습니다.

## 8. 멈춤 조건

`north_go`는 [_decision()](../north_go.py) 안에서 아래 조건이 만족되면 멈춥니다.

1. latest frame에서 `detect_arcane_end()`가 성공하면 즉시 멈추고 `end_latest_frame` 상태를 반환합니다.
2. 그 외에는 fresh한 slow payload의 `end`가 `None`이 아닐 때 멈추고 `end` 상태를 반환합니다.

그때 하는 일:

1. Arcane end를 감지했다는 로그를 남깁니다.
2. `session.request_stop()`을 호출합니다.
3. decision callback은 위 감지 경로에 해당하는 상태를 반환합니다.

## 9. Steering 단계

최종 candidate가 정해진 뒤에는 [_steer_arcane_movement()](../north_go.py)가 실제 커서 위치를 결정합니다.

현재 하는 일은 아래와 같습니다.

1. 필요하면 floor-guided ratio 보정을 합니다.
2. family가 west/east이면 `ARCANE_CURSOR_RADIUS_SCALE = 7.9 / 7.0`으로 중심에서 바깥쪽으로 ratio를 확대합니다.
3. 최종 ratio로 커서를 이동합니다.
4. side family에서 north로 다시 열릴 때는 짧은 fast reacquire를 사용합니다.
5. chest/coffin, shrine, teleporter hover가 걸리면 nudge로 살짝 피합니다.

steering은 커서 위치를 정하며, 이동 입력은 별도로 설정된 movement key를 통해 보냅니다.

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
3. slow vision으로 end / monster / loot / hover를 계속 계산합니다.
4. north gate가 열려 있으면 north family를 우선합니다.
5. north gate가 닫히면 west vs east를 gate로 먼저 비교합니다.
6. 선택된 family 내부에서 가장 좋은 candidate를 사용합니다.
7. hysteresis로 family/candidate 흔들림을 줄입니다.
8. steering 단계에서 floor-guided 보정, side 반경 확대, hover 회피를 적용합니다.
9. latest frame 또는 fresh slow 결과에서 공용 Arcane end를 감지하면 멈춥니다.

## 12. 참고 메모

- 이 문서는 현재 generic goal center 및 chest/coffin end 구조를 기준으로 작성되었습니다.
- 이후 `south_go`, `east_go`, `west_go`도 같은 terminal helper를 재사용할 수 있습니다.

## 13. 응답성 튜닝

현재 [north_go.py](../north_go.py)의 응답성 관련 값은 다음과 같습니다.

- `ARCANE_RUNTIME_CAPTURE_FPS = 30.0`
- `ARCANE_RUNTIME_FAST_INTERVAL = 0.002`
- `ARCANE_RUNTIME_DECISION_INTERVAL = 0.002`
- `ARCANE_CURSOR_FAST_SETTLE = (0.0, 0.004)`
- `ARCANE_HOVER_RELEASE_SETTLE = (0.0, 0.006)`
- `ARCANE_DECISION_STEP_SETTLE = (0.0, 0.002)`
- `ARCANE_FAST_SEQUENCE_GAP_LIMIT = 5`
- `ARCANE_FAST_TREND_INTERVAL = 3`

decision은 cached fast payload가 있을 때 sequence gap이 한도를 넘는 새 결과를 건너뜁니다. 초기 payload가 없을 때는 첫 결과를 받아들이므로 모든 stale 결과를 거부하는 규칙은 아닙니다. fast age에 따른 steering/family 전환과 slow freshness에 따른 hover/end 판단은 별도로 확인해야 합니다.

`_fast_vision()`은 trend를 최초 및 sequence 간격마다 계산하고 캐시를 재사용합니다. 같은 fast map을 direction/family scoring에 사용하므로 비용 분석 없이 계산을 중복 추가하지 않습니다.

side-stuck 탈출은 다음 값으로 제어합니다.

- `ARCANE_SIDE_STUCK_FRAME_CHANGE_THRESHOLD = 4.5`
- `ARCANE_SIDE_STUCK_BREAK_STEPS = 3`
- 정체가 누적되면 반대 side family로 한 tick 전환합니다.

튜닝은 다음 순서로 진행합니다.

1. 같은 attempt의 녹화와 `frame_age`, `fast_age`, `slow_age`, `fast_gap`, `fast_proc`, `turn_reason`을 비교해 캡처 지연, scoring 비용, stale 판단을 구분합니다. decision 처리 시간은 필요하면 별도 계측합니다.
2. freshness gate와 cached payload 사용이 의도대로 동작하는지 먼저 확인합니다. slow 데이터가 오래되었을 때 hover/end 판단에 적용되는 조건도 함께 확인합니다.
3. capture FPS를 조정할 때는 실제 전달된 freshness와 부하를 비교합니다. FPS 증가만으로 응답성 개선을 판정하지 않습니다.
4. `_fast_vision()` 비용이 병목이면 trend 주기, map 재사용, candidate/path sampling을 한 그룹씩 조정합니다. 최신 frame 소비와 backlog도 함께 점검합니다.
5. timing 변경과 side-stuck 탈출 변경은 따로 비교합니다. 불안정해지면 변경한 그룹부터 되돌리고 같은 시작 상태의 녹화로 차이를 확인합니다.

## 14. Tuning Runner And Durable Logs

The north-go tuning flow now supports repeated attempts from one GUI start. Paths, recording controls, retention, and log defaults are owned by the [system guide](../../../../../config/system/system.md).

- The GUI `Repeat Count` field is used for north-go reruns.
- Each attempt creates a video when `auto_record_runs` is enabled, under the configured recording directory (default `./recordings/`).
- Each attempt also writes a JSON summary in that recording directory.
- Route telemetry is submitted to the bounded logger and rotated files under the configured logging directory (default `./logs/`). Queue saturation can drop low-priority records; priority 0 currently falls back to synchronous writing. This is not lossless or fully isolated recording.
- The GUI log is a rolling live view. Retained file segments have their own limit and do not preserve all messages indefinitely.

The system guide owns numeric logging/GUI defaults and runtime recording retention. For north-go videos:

- failed or interrupted runs are kept by default
- a small number of early reference runs can also be kept
- ordinary successful runs can be pruned after their summary is written

Those runtime flags do not replace the [development-session cleanup rule](../../../../../docs/project/developer-guide.md#evidence-handling). Long-run total quotas and recording/control isolation remain [PRD-0008](../../../../../docs/plans/prd/prd-0008-realtime-runtime-and-recording-retention.md) proposals.

Turn-tuning changes:

- west/east side travel uses the configured `movement_travel_mode` (default `hold`) during stable side traversal
- side-to-north turns require short confirmation instead of changing on one noisy frame
- north-go per-step telemetry now records turn reason, key-state changes, and frame freshness data for later comparison with recordings
