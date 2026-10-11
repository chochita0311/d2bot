# SPEC-0009: 확인 결과 기반 Summoner 방 제어

- ID: `spec-0009`; Status: `implemented`; Run: `run-20261009-02`; Attempt: `1`
- Feature: [FEAT-0010](../feature/feat-0010-summoner-cycle-contract.md)
- Profile: `foundation-contract`; Evaluators: Contract, Functional; Created / Updated: `2026-10-09`

## Producer And Consumer

`SummonerCycle`은 단일 owner가 step/tick하는 입력 없는 planner다. 호출자는 character ID·고유한 유한 room ID 목록·monotonic 시작 시각과 모든 시한을 명시한다. `CycleObservation`은 기존 ObservationStamp, 관찰 세대, 의미 event와 연결한 command ID를 가진다. 입력 전송 보고는 이 관찰의 생산자가 될 수 없다.

첫 시작은 현재 캐릭터/예정 방 맥락의 새 `character_select` 화면만 허용한다. frame 순번과 monotonic 시각은 cycle 전체에서 유지한다. 이후 결과는 현재 pending 명령 ID, 더 큰 frame 순번, 명령 이후 관찰 시작 시각, 현재 이상 세대, 승인한 screen age를 모두 만족해야 한다. 이전 명령의 늦은 결과는 진행시키지 않는다. 창/이동 세대가 증가하면 이전 세대 결과도 거부한다. 결과를 기다리는 동안 일반 명령을 반복 전송하지 않는다.

단계별 의미는 다음과 같다: create_room→act1_ready, enter_arcane→arcane_ready, prepare_field→field_ready, search_wing→wing_empty_confirmed 또는 summoner_alive, fight_boss→summoner_dead_confirmed, inspect_loot→loot_complete, return_hub→hub_ready, return_act1→act1_ready, restock→restock_complete, exit_room→character_select. field_ready는 현재 라벨·버프 효과·장비 복귀·벨트/자원 조건의 확인이고, wing_empty_confirmed는 끝 지점과 해당 날개의 보스 부재를 확인한 의미다. 일반 goal 템플릿이나 template 미검출을 이 event로 바꾸면 안 된다. 사망과 획득의 독립 증거는 각각 기존 전투/전리품 계약을 따른다.

각 명령 발행 전 입력 adapter는 유지 키를 해제하고 최신 포커스/시계/세대를 재검사해야 한다. planner의 `release` directive는 시한·포커스/중단/불확실 상태에서 반드시 실행해야 하는 입력 해제 요청이다. planner 자체는 OS 입력을 보내거나 해제를 보증하지 않는다. 이 단계는 adapter나 GUI의 live 연결을 포함하지 않는다.

## Flow And Results

빈 날개 뒤 중앙 귀환을 확인한 다음 순서의 날개를 검색한다. 네 날개 모두 긍정 부재이면 정상 성공 대신 review_required로 끝낸다. 보스가 살아 있으면 다른 날개를 탐색하지 않고 전투→긍정 사망→전리품 확인으로 진행한다. 열쇠 드롭 0도 확인된 loot_complete이면 중앙→Act 1→재정비→종료로 진행한다.

loot_complete의 열쇠 드롭/획득 수는 정수이며 0≤획득≤드롭이다. 미회수 열쇠가 남으면 정상 마감으로 바꾸지 않고 검토 요청으로 끝낸다. 정상 결과는 선택 복귀 확인 뒤 한 번만 기록한다. 그 뒤에만 다음 room ID로 create_room을 요청하며 유한 목록이 끝나면 추가 생성하지 않는다. 실패 결과도 요청한 방당 한 번만 기록하며 정상 성공과 구별한다. `room_entered`는 생성 뒤 Act 1 긍정 도착 여부다. 생성 전 실패를 실제 게임에 들어간 1방으로 세면 안 된다.

screen age·각 단계·방·전체 시한은 양수/유한, count는 유한·고유 목록이다. 시작 화면 미확인은 release하고 진행 중 결과 미확인·과거 결과는 wait한다. wait는 새 입력 허가가 아니며 진행 중 field executor가 자체 최신성/유지 시한을 계속 검사해야 한다. stale/wrong context/포커스 상실/stop/clock 역행/명시적 실패는 pending 명령을 취소하고 review_required로 종료한다. 실패 뒤 자동 방 생성·입력 재시도는 없다. 사용자가 확정한 안전 종료 후 사람 확인은 후속 실제 executor에서 연결한다. 사망 뒤 자동 재정비를 추가하지 않는다.

## Verification

입력 없는 의미 관찰 재생으로 네 날개 순서, 첫/마지막 날개 보스, 0/복수 열쇠, 미회수, 단계별 잘못된/과거 확인, 종료 중복, 각 시한, stop/focus/clock/context 경계를 검사한다. 새 Python Black/compile와 전체 unittest 회귀를 수행한다. 게임 입력은 보내지 않는다.
