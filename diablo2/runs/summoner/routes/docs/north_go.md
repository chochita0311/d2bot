# north_go 동작 설명

이 문서는 [north_go.py](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py)의 현재 동작을 코드 순서대로 설명합니다.

기준 좌표 해석은 다음과 같습니다.

- `north` = `2 o'clock`
- `left` = `10 o'clock`
- `right` = `4 o'clock`

`north_go`는 단순히 한 번 목표점을 찍고 끝나는 함수가 아니라, 실시간 비전 결과를 계속 받아서 방향을 다시 평가하고 커서를 다시 조준하는 작은 제어 루프입니다.

## 1. 시작 단계

엔트리 함수는 [run_arcane_north_go()](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L115) 입니다.

처음에 하는 일은 다음과 같습니다.

1. 캐릭터가 이동 스킬을 사용할 수 있는지 확인합니다.
2. `prepare_arcane_hub_start(..., "north")`로 Arcane hub 시작 상태를 맞춥니다.
3. hub focus ratio를 읽어서 커서를 그 위치로 맞춥니다.
4. movement intent를 `REPOSITION`으로 한 번 실행해 시작 위치를 정렬합니다.
5. belief state를 만들고 현재 wing을 `north`로 commit합니다.

즉 이 단계의 목적은 "북쪽 테스트를 시작하기 전에 hub 기준점을 먼저 맞춘다"입니다.

## 2. 내부 제어 상태

`run_arcane_north_go()` 안에는 `control` 딕셔너리가 있고, 여기서 현재 루프의 상태를 기억합니다.

중요한 값은 다음과 같습니다.

- `last_direction_key`
  - 직전에 선택한 후보 key입니다.
- `last_direction_ratio`
  - 직전에 실제로 조준한 ratio입니다.
- `route_family`
  - 현재 큰 방향 family입니다.
  - `north`, `left`, `right` 중 하나입니다.
- `route_family_steps`
  - 같은 family를 몇 tick째 유지 중인지 기록합니다.
- `last_fast_payload`, `last_slow_payload`
  - 가장 최근 fast/slow 비전 계산 결과를 저장합니다.

이 값들이 있어야 hysteresis와 continuity bonus가 작동합니다.

## 3. Runtime 구조

실제 반복 루프는 `RealtimeVisionRuntime`으로 돌고, 안에 세 가지 콜백이 들어갑니다.

- `_fast_vision`
- `_slow_vision`
- `_decision`

흐름은 크게 다음과 같습니다.

1. fast 비전이 방향 관련 점수를 자주 계산합니다.
2. slow 비전이 monster, loot, hover blocker, terminal을 비교적 천천히 계산합니다.
3. decision 단계가 두 결과를 합쳐 이번 tick의 실제 이동 방향을 정합니다.

## 4. Fast 비전 단계

[run_arcane_north_go()](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L115) 안의 nested 함수 `_fast_vision()`이 담당합니다.

이 단계에서 하는 일은:

1. 현재 프레임으로 `fast_maps`를 만듭니다.
2. 모든 방향 후보에 대한 vote를 계산합니다.
3. `north_open` gate 신호를 계산합니다.
4. 최근 프레임 차이를 보고 progress change / progress trend를 계산합니다.

즉 fast 비전은 "지금 어디로 가는 게 제일 좋아 보이는가"를 빠르게 계속 갱신하는 역할입니다.

### 4-1. fast_maps

[ _build_arcane_fast_maps() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L480)는 현재 프레임을 축소한 뒤 두 가지 mask를 만듭니다.

- `floor_mask`
  - Arcane floor gray palette와 가까운 픽셀
- `star_mask`
  - star/void palette와 가까운 픽셀

이 두 mask는 이후 모든 방향 점수 계산에 재사용됩니다.

### 4-2. 방향 후보 vote

[ _score_arcane_direction_candidates() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L491)가 후보별 점수를 계산합니다.

후보는 [ARCANE_DIRECTION_CANDIDATES](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L97)에 정의되어 있습니다.

- north family
  - `north_primary`
  - `north_soft`
  - `north_sharp`
  - `north_recover`
- left family
  - `left_turn`
  - `left_soft`
- right family
  - `right_soft`
  - `right_turn`

각 후보 score는 대략 아래 3개를 더해서 만듭니다.

- `openness`
  - 그 방향 경로가 floor-like 한지
- `candidate.bias`
  - 후보별 사전 선호도
- `continuity_bonus`
  - 직전 방향과 가까울수록 조금 더 가산

즉 지금의 점수 구조는:

`score = openness + bias + continuity_bonus`

입니다.

### 4-3. ray 경로 점수

[ _score_arcane_direction_path() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L603)는 후보 하나를 평가하는 핵심 함수입니다.

이 함수는:

1. 화면 중심 zero point에서 candidate ratio까지 직선을 하나 잡습니다.
2. 그 직선 위 `0.45`, `0.65`, `0.85` 위치를 샘플합니다.
3. 각 샘플 주변 패치에서 `floor_mask`와 `star_mask` 평균을 계산합니다.
4. 최종적으로:

`openness = floor_ratio - (star_void_ratio * 0.85)`

를 만듭니다.

이 구조의 장점은 단순하고 빠르다는 점입니다.

이 구조의 한계는 실제 길이 곡선인데 ray는 직선이라는 점입니다.

즉 curved bend에서는 "후보점 자체는 맞아도, 그 후보까지 가는 직선 샘플링이 실제 길을 잘 못 읽을 수 있음"이 현재 구조의 약점입니다.

## 5. north_open gate

`north_go`에서 가장 중요한 우선순위는 `north_open`입니다.

[ _score_arcane_north_open_signal() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L628)는 `2 o'clock`이 열렸는지 따로 판정합니다.

이건 left/right 평가처럼 ray 직선을 보지 않습니다.

대신:

1. 화면의 `upper-right quarter`를 별도 영역으로 잡고
2. [ARCANE_NORTH_OPEN_CIRCLE_PROBES](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L67)에 있는 원형 probe 3개를 검사한 다음
3. 그중 가장 floor-like 한 값을 `north_open`으로 사용합니다.

즉 `north_open`은:

- "2시 방향 창문이 열렸는가?"를 보는 gate

이고,

`left/right` 후보 점수는:

- "10시 또는 4시로 가는 경로가 더 좋아 보이는가?"를 보는 vote

입니다.

이 둘은 다른 레이어입니다.

### 5-1. north_open의 의미

[ARCANE_NORTH_OPEN_FLOOR_RATIO](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L58)가 기준값입니다.

- `north_open >= ARCANE_NORTH_OPEN_FLOOR_RATIO`
  - `2시 방향이 열렸다`
- `north_open < ARCANE_NORTH_OPEN_FLOOR_RATIO`
  - `2시 방향이 닫혔다`

로 봅니다.

## 6. Slow 비전 단계

`_slow_vision()`은 상대적으로 무거운 판단을 담당합니다.

여기서 보는 것은:

- `north_terminal`
- `loot_label`
- `monster_hit`
- `hover_blocker_kind`

### 6-1. terminal 감지

[ _detect_arcane_north_terminal() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L721)는 북쪽 루트 끝에 도달했는지 감지합니다.

방법은:

- summoner north template
- summoner clue
- without-summoner north template

를 순서대로 보는 식입니다.

### 6-2. hover blocker 감지

[ _detect_arcane_hover_blocker() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L437)는 현재 커서가 hover 때문에 막히는 오브젝트를 찾습니다.

반환값은:

- `"chest"`
- `"teleporter"`
- `None`

중 하나입니다.

이 값은 steering 단계에서 더 멀리 nudge할지, 그냥 유지할지를 결정하는 데 쓰입니다.

## 7. decision 단계

실제 방향 결정은 nested 함수 `_decision()`에서 합니다.

이 함수는 매 tick마다 다음 순서로 생각합니다.

### 7-1. user interrupt 확인

사용자 입력이 들어왔으면 즉시 중단합니다.

### 7-2. stale frame 여부 확인

`fast_age_ms`, `slow_age_ms`를 계산해서

- fast 결과가 너무 오래됐는지
- slow 결과가 너무 오래됐는지

를 판단합니다.

관련 상수는:

- [ARCANE_FAST_SWITCH_LIMIT_MS](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L48)
- [ARCANE_FAST_STEER_LIMIT_MS](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L50)
- [ARCANE_SLOW_STALE_LIMIT_MS](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L52)

#### stale-fast hold

fast 결과가 너무 오래됐으면 새 결정을 하지 않고, 마지막 방향을 그대로 유지합니다.

이 구간에서는:

- `last_direction_key`
- `last_direction_ratio`

를 fallback으로 사용합니다.

즉 "새 frame을 믿기 어려우니, 방금 가던 쪽을 유지"하는 모드입니다.

### 7-3. terminal / monster / loot 처리

slow 결과가 fresh할 때:

- terminal이 보이면 종료
- monster가 보이면 잠깐 pause
- loot가 보이면 잠깐 pause

합니다.

즉 방향 결정보다 먼저 "지금 당장 멈춰야 하는 이유가 있나"를 봅니다.

### 7-4. 방향 후보 선택

pause할 이유가 없으면 `_choose_arcane_direction()`으로 들어갑니다.

## 8. 최종 방향 선택 규칙

[ _choose_arcane_direction() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L523)가 최종 후보 하나를 고릅니다.

현재 로직은 다음 순서입니다.

### 8-1. north gate 우선

먼저 `north_open`을 읽습니다.

`north_open`이 열려 있으면:

- `north` family 후보 중 최고점 하나를 즉시 반환합니다.

즉 `2시 방향이 열려 있으면 north 우선`이 현재 최상위 규칙입니다.

### 8-2. north가 닫히면 left vs right 비교

`north_open`이 닫혔으면:

- left family 최고점 1개
- right family 최고점 1개

를 비교해서 더 높은 family를 고릅니다.

여기서 중요한 점은:

- family 전체 평균을 쓰는 게 아니라
- 각 family 안의 최고 후보 1개를 비교한다

는 점입니다.

### 8-3. family hysteresis

이미 `left`나 `right`를 가는 중이면, 새 family가 조금만 좋아서는 바로 바꾸지 않습니다.

기준은 [ARCANE_FAMILY_KEEP_MARGIN](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L60) 입니다.

즉:

- 현재 family보다 새 family가 아주 살짝 좋다
  - 기존 family 유지
- 충분히 더 좋다
  - family 전환 허용

### 8-4. previous vote hysteresis

family 수준 말고 candidate 수준에서도 한 번 더 안정화합니다.

기준은 [ARCANE_VOTE_KEEP_MARGIN](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L59) 입니다.

즉:

- 새 후보가 이전 후보보다 아주 조금만 좋다
  - 이전 후보 유지

이 로직이 있어야 좌우로 계속 미세하게 흔들리는 것을 줄일 수 있습니다.

## 9. 실제 steering 단계

최종 후보를 고른 뒤에는 [ _steer_arcane_movement() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L403)로 실제 커서를 조준합니다.

이 함수는 다음 순서로 동작합니다.

1. 필요하면 floor-guided ratio로 base ratio를 조금 보정
2. family가 `left/right`면 중심 쪽으로 반경 축소
3. 커서를 그 ratio로 실제 이동
4. `fast_reacquire`면 아주 짧게 settle
5. hover blocker가 있으면 nudge offset으로 추가 회피

### 9-1. floor-guided 보정

[ _resolve_arcane_floor_guided_ratio() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L454)는 base ratio 주변 몇 개 후보를 더 보고,

- 가장 바닥처럼 보이는 점

으로 조준점을 조금 옮깁니다.

즉 "정해진 target point를 그대로 쏘지 말고, 그 주변에서 실제 floor가 더 잘 보이는 점으로 미세 보정"하는 단계입니다.

### 9-2. side family 반경 축소

family가 `left` 또는 `right`이면 [ARCANE_CURSOR_RADIUS_SCALE](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L63)을 적용해 목표를 화면 중심 쪽으로 당깁니다.

이유는:

- `2시`는 먼 곳을 찍어도 직진 성향이라 괜찮지만
- `10시`, `4시`는 bend에서 너무 멀리 찍으면 제어가 거칠어지기 때문입니다.

### 9-3. 빠른 north 재진입

직전 family가 `left/right`였는데 이번에 `north`가 다시 열린 경우,

- `quick_reopen_steer = True`

가 되고,

- [ARCANE_NORTH_REOPEN_FAST_SETTLE](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L61)

을 사용해 빠르게 `2시`로 재조준합니다.

즉 side branch에서 다시 `2 open`을 찾았을 때 커서가 한 박자 늦지 않도록 만든 장치입니다.

### 9-4. hover blocker 회피

`hover_blocker_kind`가 있으면:

- chest
- teleporter

종류에 따라 다른 nudge offsets를 사용합니다.

현재는 chest 쪽을 더 크게 피하도록 되어 있습니다.

## 10. progress change / trend

`north_go`는 현재 "실제로 움직이고 있는가"를 화면 차이로 봅니다.

관련 함수는:

- [ _measure_arcane_progress_change() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L746)
- [ _measure_arcane_progress_trend() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L755)
- [ _arcane_progress_roi() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L768)

핵심 아이디어는:

- 전체 화면 대신 중앙 ROI만 비교
- 연속 프레임 grayscale 차이 평균을 progress로 간주

입니다.

이 값은 현재 decision 로그의 `frame_change`에 반영되고, 향후 bend dead-end 판단을 더 세게 만들 때도 중요한 기초 지표가 됩니다.

## 11. 지금 코드의 실제 우선순위

현재 `north_go`를 아주 짧게 요약하면:

1. 시작 위치를 hub 기준으로 정렬
2. fast 비전으로 후보 vote와 `north_open`을 계산
3. slow 비전으로 terminal / monster / loot / hover를 계산
4. stale이면 마지막 방향 유지
5. `north_open`이 열리면 무조건 north family 우선
6. north가 닫히면 left vs right 최고 후보 비교
7. family hysteresis와 previous-vote hysteresis로 작은 흔들림 방지
8. 최종 후보를 floor-guided + radius scale + hover 회피를 거쳐 실제 조준
9. terminal이 보이면 종료

## 12. 현재 구조의 강점과 약점

### 강점

- 빠르다
- 구조가 단순하다
- `north gate`와 `left/right vote`가 분리돼 있어 해석이 쉽다
- hysteresis가 있어 완전 난수처럼 흔들리지는 않는다

### 약점

- `10/4` candidate 평가는 여전히 직선 ray 샘플 기반이다
- 실제 Arcane bend는 곡선인데, 현재 평가는 직선 경로를 가정한다
- 그래서 curved branch에서 `left <-> right`가 뒤집히는 장면이 생길 수 있다
- 특히 한 자리에서 계속 `10`과 `4`가 둘 다 그럴듯해 보이는 장면에 약하다

즉 지금 `north_go`의 핵심 병목은:

- north gate 자체보다는
- side family를 고른 뒤 그 family 내부 bend를 어떻게 더 잘 따라갈지

에 있습니다.

## 13. 코드 읽기 추천 순서

처음 읽을 때는 아래 순서로 보면 이해가 가장 빠릅니다.

1. [run_arcane_north_go()](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L115)
2. [ _choose_arcane_direction() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L523)
3. [ _score_arcane_direction_candidates() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L491)
4. [ _score_arcane_direction_path() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L603)
5. [ _score_arcane_north_open_signal() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L628)
6. [ _steer_arcane_movement() ](/D:/python/d2bot/diablo2/runs/summoner/routes/north_go.py#L403)

이 순서로 보면:

- 전체 흐름
- 방향 선택 규칙
- 후보 점수 계산
- north gate
- 실제 조준

이 자연스럽게 이어집니다.
