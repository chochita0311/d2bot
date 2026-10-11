> Historical reference through 2026-10-10. The words current/현재 below describe that version. Latest GUI, automatic buff and conditional ALT contracts are owned by [Summoner Run](summoner-run.md); actual evidence and failures are owned by the [integration run](../plans/run/run-20261009-07-summoner-live-integration.md). The old journal/red-portal return is historical and does not replace the current central-waypoint goal.
# Summoner Run Spec

The target payload describes the full run. Executable coverage is listed under Current staged implementation.

The [Summoner experiment plan](summoner-experiment-plan.md) maps the PRDs for room repetition, Flash survival/buffs, all four wings, nearby-enemy combat, shared loot/identification, and town maintenance. On 2026-10-09 the owner authorized requirements, implementation, and supervised verification, with 27 normal finishes out of 30 rooms as the first complete-run acceptance target. Failure requires safe exit and human review. This authorization does not establish completed behavior; this reference retains the observed timeline and executable coverage below.

The separately approved [town room loop](town-room-loop.md) has finite create→Act 2 town→Act 1→exit→recreate behavior. It does not invoke the Summoner combat payload; its own run record distinguishes that runtime evidence from the staged payload below.

현재 통합 개발 진입점은 다음과 같다. `--live`를 생략하면 입력하지 않는다.

2026-10-10 결정에 따라 아이템 이름표는 다른 런에도 재사용하는 항상 ON 기본값이다. 게임의 ALT 표시/숨기기 토글 모드에서 새 방은 한 번 켜고 같은 방 재개는 최신 감독 `item_labels_on=true`를 요구해 다시 토글하지 않는다. 예외로 새 방 진입 뒤 ALT 전송0회임이 확인된 재개는 `source=supervised_visual`, `item_labels_on=false`, `fresh_room_default_off=true`를 함께 받고 프로그램이 한 번 켠다. 드롭이 없으면 ON 요청과 실제 표시 관찰을 구분한다. 이름표를 끈 시험은 진단이며 정상 ON 통합 시험으로 세지 않는다. 현재 공통 가림 mask와 끝의 부분 텍스처 비교를 연결했으나 일반 아이템 정체·획득 판독은 별도다. 이름표가 지도 제목 검색 영역을 가리면 Tab을 보내지 않고 확인을 보류한다.

기본 왕복이 안정되면 서모너 발견→처치→드롭 확인→증오의 열쇠가 있을 때 회수까지 포함한 새 방5회 묶음을 실행하고 실패 지점·처리 시간·런 시간을 보고 보완한 뒤 반복한다. `--north-cycles 1..5`는 현재 북쪽 개발 묶음이며 빈 끝 마감은 전체 서모너 성공과 구분한다.5회 개발 묶음은 최종30방 중27방 이상 정상 마감 기준을 대체하지 않는다. 현재 새 방 북쪽 전체 왕복·실제 처치/회수 성공은 아직 없다.

현재 연속 입력은 조준 대기와 실제 F2 전송을 따로 기록한다. 화면250ms가 끝나면 키를 해제하고, 다음 새 화면과 실제 포인터 일치가 확인된 같은 조준에만 완료된50ms 대기를 재사용한다. 명시적 해제는 조준을 폐기한다. 실제 전송 없는 정지를 길 막힘으로 분류하지 않는다. main166/167의 이전 `hold_teleport` 기록에는 실제 F2가 없었으므로 계단 착지 실패 증거로 사용하지 않는다. 수정 후 실게임 검증 여부는 통합 run이 소유한다.

개발 main은 자동지도 ON을 기본으로 확인한다. 최신 HUD·닫힌 패널·포커스 조건에서 지역 제목의 금색 글자를 읽고, 이미 켜져 있으면 Tab을 보내지 않는다. 제목이 이름표에 가린 재개는15초 이내의 최신 감독 `automap_on=true`를 별도 근거로 받아 Tab 없이 `on_supervised`로 기록할 수 있다. 이는 자동 지도 인식이 아니다. 이 근거도 없으면 최대 한 번 켜기를 시도한 뒤 양성 확인 없이는 이동하지 않는다. 이동 중 토글은 없다. `--automap preserve`는 명시적 ON/OFF 비교 시험용이며 현재 지도 표시를 보장하지 않는다. 조향은 아직 바닥/배경 대응을 사용하므로 지도 선의 경로 해석과 간섭 분리는 별도 검증 중이다.

연속 main의 버프 시한은 실제 키 요청의 단조 시각에서 계산한다. 남은 개발 구간이 있으면 입력을 해제하고 최신 `field_safety`→현재 비율 안정/근접 적 검사→프로필 전체 버프→최신 `initial_effect` 확인 뒤 같은 경로 그래프를 이어간다. 갱신에는 인벤토리 확인을 추가하지 않는다. 최대8회이며 자료 대기는300초·감독 근거는15초다. 부분 시전·효과 미확인·새로 갱신한 버프가 이미 만료된 확인은 새 시한이나 이동을 승인하지 않는다. 현재 안전/효과는 감독 근거를 받으므로 완전 자동 갱신과 구분한다.

```powershell
.\.venv\Scripts\python.exe main.py --summoner-run --live --continuous-north --field-seconds 60 --north-segments 4
```

같은 프로세스가 방 생성·성역 진입·무기 I 확인·전체 버프·감독 효과 확인·F2 연속 유지 이동을 실행한다. 연속 구간은1–60초, 한 단계 최대4구간이며 현재 HUD/발 위치·넓은 발판·꺾임을 다시 읽어 조향한다. `--no-combat`는 F4 전투 선점을 끈다. 이미 필드에 있을 때 `--return-to-hub-from south`는 같은 방 중앙 복귀이며 중앙 도착 전 새 북쪽 출발로 세지 않는다. 감독 JSON은 버프 전/후에 받고 키 조작은 프로그램이 수행한다.

정상 interval은 동일 날개·복귀 모드와0.5초 미만의 직전 결과로 자동 연결하며 실제121/127/130/132에서 이어졌다. 다음 구간도 새 HUD·발밑·발판·포커스와 실제 S 요청+140초 시한을 요구한다.122는 세 상자 끝 확인 뒤 같은 방 복귀를 자동 시작했다. `--all-wings`는 이 연속 모드를 포함하며 중앙 정지의 최신0.5초 미만 결과 뒤 북→동→남→서로 연결한다. 네 빈 끝은 검토 상태이며 정상 완료가 아니다. 다중 문양 중앙 판독·추가 시전 종료를 기다리는 정지 시간·일지/룬 목표 정지를 연결했지만 새 중앙 도착·실제 네 방향 연결과 서모너 처치/회수는 아직 미검증이다.

Act 2 시작은 악몽 마을→종료→지옥 Act 1 초기화를 사용하고 준비 방을 완료 수에 넣지 않는다. 개발 중 일반 적/경로 미확인은 입력 해제 뒤 같은 방에서 보정한다. 준비 실패·경로 정지 후에도 명시적 중단·포커스 인계 또는 실제 자원 저하·2초 지속 HUD 미확인까지 생존 감시를 유지한다. 후자의 위험에서 안전 종료하며125의 실제 마나 고갈 후 선택 화면을 확인했다. 이는 일반 사망 방지 통과가 아니다.

현재 목표 발판은 일지/룬 또는 검토한 서로 다른 네 룬 외형·상대 배치로 확인한다. 전투 ON에서 현재 로브 외형 후보가 보이면 제한 접근/노바를 실행하지만 후보 소실을 사망으로 확정하지 않으며 실제 처치·열쇠 회수 승인 연결은 남아 있다. 대기 중 외형 미등록 적은 근접 지점의 실제 커서와 후속 현재 적 체력 패널을 확인하는 호버 보조 방어를 추가했다. 전투 OFF에서는 이 호버도 실행하지 않는다. 178에서 대기 호버 방어,187에서 이동 중 현재 적 패널 확인 뒤 노바3개 구간을 실제 소비했다. 녹색 효과는 이동 중 호버 후보만 만들며 적 패널의 미검출은 clear가 아니다. 모든 근접 적 감지·처치 확인이나 먼 적 접근/원위치 복귀의 완료는 아니다.

기존 `--north-casts 1..12` 단일 시전·12초 모드는 구간 사이 감독 확인을 받는 별도 회귀 진입점이다. GUI·실제 네 방향·처치/회수·140초 자동 갱신·정상 반복과30방 수용은 남아 있으며 두 모드 모두 수용 방으로 세지 않는다. 실제 결과는 [최신 실행 기록](../plans/run/run-20261009-07-summoner-live-integration.md)이 소유한다.

## Goal

Describe the observed Summoner Run as a payload-first target spec, and distinguish it from the narrower executable stages below.

The owner's confirmed normal loop is room creation → Arcane Sanctuary → wings at 2, 4, 8, then 10 o'clock → kill the encountered Summoner → collect any key and approved loot → return to the central Arcane waypoint → Act 1 town → exit → repeat. Finding the Summoner ends further wing search, so a run may search one to four wings. Belt inspection must finish with verified closure before movement. Existing executable stages below do not yet complete this loop.

## Payload boundary

The target payload covers the body of the run:

- start after the room is already created and the character can act in town
- finish when the target work is done and the character is back in a safe handoff state
- keep reusable room creation and room exit outside this module

Reusable wrapper shape:

- create room via `run_lifecycle.create_room(...)`
- execute the Summoner payload
- exit room via `run_lifecycle.exit_room(...)`
- repeat until the run budget is exhausted

Room creation and exit already exist in `diablo2/actions/run_lifecycle.py`. The Summoner orchestrator currently attaches room creation before its staged payload; complete payload handling and the normal post-run wrapper are still unfinished.

## Observed payload timeline

This is a historical observation. Its journal/red-portal/Canyon return differs from the newly confirmed central-waypoint return above.

- `8s-10s`: Act 1 town spawn
- `9s-10s`: tap `ALT` once to enable item labels
- `10s-13s`: move to waypoint and open it
- `13s-15s`: switch to Act 2 and click `비전의 성역`
- `15s-16s`: loading
- `16s-70s`: hunt one Arcane Sanctuary wing
- `70s-78s`: kill `소환사`
- `76s-81s`: loot scan, no `key of hate` seen in this run
- `80s-84s`: click `호라존의 일지`
- `84s-86s`: use red portal
- `85s-86s`: Canyon arrival
- `93s-105s`: Act 1 town again, post-run handling visible

## Payload states

### 1. `act1_town_loaded`

Expected evidence:

- town spawn is complete
- life and mana orbs are visible
- movement and input are available

Success action:

- prepare the run body from an in-game starting state

### 2. `labels_enabled`

Expected evidence:

- dropped item labels remain visible without holding `ALT`

Success action:

- move toward the waypoint

### 3. `waypoint_open`

Expected evidence:

- waypoint menu is visible
- Act tabs are available

Success action:

- switch to Act 2 and click `비전의 성역`

### 4. `arcane_loaded`

Expected evidence:

- Arcane Sanctuary area name or recognizable geometry is visible

Success action:

- commit to one wing and start the live coordinator loop

### 5. `wing_search_started`

Expected evidence:

- route progress continues along one wing
- combat happens only when blockers or threats force it

Success action:

- continue coordinator decisions until the Summoner platform is found

### 6. `summoner_detected`

Expected evidence:

- `소환사` / Summoner platform is visible
- boss identity or platform geometry is confidently confirmed

Success action:

- kill the target safely

### 7. `summoner_killed`

Expected evidence:

- target is dead
- post-kill drops are visible

Success action:

- run the loot decision pass

### 8. `loot_scan_complete`

Expected evidence:

- visible drops have been kept or ignored
- `key of hate` decision is complete

Success action:

- if the journal is available, click `호라존의 일지`

### 9. `journal_clicked`

Expected evidence:

- journal interaction succeeded
- red portal is available

Success action:

- enter the portal

### 10. `canyon_loaded`

Expected evidence:

- Canyon / portal destination is loaded

Success action:

- return to town and settle into a safe post-run handoff state

### 11. `post_run_complete`

Expected evidence:

- no target actions are pending
- the payload is ready for the reusable room-exit wrapper

Success action:

- hand off to room-exit logic when that wrapper is attached

## Coordinator behavior inside the payload

Inside `wing_search_started` through `loot_scan_complete`, the payload should behave like one coordinator loop following the [shared priority order](run-coordination.md#priority-order).

Do not split the middle of the run into isolated scripts that compete with each other.

## Profile resolution

The Summoner payload should resolve its own profile id, `summoner`, directly from `run_profiles`.

That keeps the run catalog reusable while still letting the Summoner payload pull together:

- hunting rules
- loot rules
- life rules
- run-specific completion rules

## Current staged implementation

[FEAT-0013](../plans/feature/feat-0013-arcane-perceptual-navigation.md) provides current-terrain search, a shared survival/buff runtime adapter, and input-free replay. [Navigation guidance](arcane-navigation.md) defines consumers and calibration. The supervised `main.py --summoner-run` path connects current feet/reviewed camera, terrain, terminal, hub and HUD observations with continuous navigation and bounded nearby Nova. Initial safety/effect evidence still comes from the supervisor; complete combat/loot, automatic140-second renewal and independent-room acceptance remain incomplete. The GUI retains the legacy stages below. NPC hover belongs only to town handling.

The current executable Summoner run stage is intentionally narrower than the full payload timeline above.

Right now the orchestrated flow is:

1. `make_room`
2. `arcane_entry`
3. `enable_labels`
4. `buff_before_run`
5. `north_go`

`Summoner Run` executes this sequence once. `North Go Test` repeats the entry/route sequence, optionally records attempts, writes outcome summaries, and exits to character select between attempts. Its former settings are preserved under [Removed runner settings](#removed-runner-settings); they no longer configure the current GUI.

Reaching the route goal is not evidence that the Summoner was killed or that the key decision is complete. Combat, survival care, journal/portal interaction, and complete post-run handling remain target behavior.

[FEAT-0014](../plans/feature/feat-0014-summoner-platform-encounter-evidence.md) adds current rune/journal floor approach, bounded Nova on a current appearance or an actual hover-confirmed hostile panel, and at most24 before/combat/after frames. The encounter report links each image to its observation sequence, monotonic capture time and phase, including frames whose HUD prevented input. These are review evidence; Summoner identity/death, acquired loot and room completion remain unconfirmed. After screen recovery, main215 dispatched one approach with no dedicated Nova. Main219 recognized the revised glyph-slot layout immediately but its later approach was not sent; the report does not distinguish which dispatch guard failed. These outcomes remain kill_unconfirmed.

[FEAT-0003](../plans/feature/feat-0003-character-survival-state.md) adds the shared character policy and input-free buff/belt state contract. Flash's configured entry sequence follows the latest owner instruction; policy fields are documented in [characters.md](../../config/characters/characters.md#survival). The legacy GUI stages have not connected the complete recovery/rebuff contract. Supervised main consumes live current/max HUD ratios and belt-closure observations, and preserves HUD monitoring during field pauses; complete live replenishment and renewal remain open.

[HUD/belt observation](survival-observation.md) reads same-frame current/max pairs and expanded slots for the reviewed native window layout, through the input-free CLI and supervised main. It passed bounded town checks including one empty slot and a closed belt. HUD readings alone supply no field/clear-surroundings or buff confirmation. Main's paused guard may use fresh nearby candidates and current feet for bounded stationary F4 while the original buff deadline remains valid; it does not yet approach farther enemies and return to the route point.

[FEAT-0006](../plans/feature/feat-0006-field-buff-confirmation.md) adds shared ordered buff confirmation and positive belt-closure gating. One supervised Arcane central-waypoint sequence supplied buffed HUD references and returned through the central waypoint to Act 1. Supervised main integrates the sequence and original cast deadlines, but the initial safety/effect evidence producer remains the supervisor. Automatic effect production and140-second renewal are not live-complete. [Field-buff guidance](field-buffs.md) owns the consumer contract.

[FEAT-0007](../plans/feature/feat-0007-arcane-north-encounters.md) adds input-free shared region candidates, floor/void patch evidence, explicit personal radius and positive linked life-state contracts. Supervised north encounters supplied Ghoul Lord/Hell Clan alive/corpse and shrine-negative assets, plus one mana-triggered potion recovery. [Encounter guidance](encounter-observation.md) owns the APIs and limits. Pose/HUD misses and expired buffs during supervised return prevent treating this as completed live safety or automatic combat; [the run](../plans/run/run-20261005-05-arcane-north-encounters.md) returned to spec for real-time supervision/integration.

[FEAT-0004](../plans/feature/feat-0004-class-casting-rules.md) adds shared class/skill reference lookup with individual FCR configuration. Flash's Sorceress FCR 105 selects 8 reference frames. [Common field control](field-control.md) now consumes that lookup with producer-confirmed arrival delays for adaptive aim timing. The current north-route executor still uses its existing movement controls; automatic arrival/readiness producers and staged route integration remain open in [the current run](../plans/run/run-20261005-06-field-control-loop.md).

## Removed runner settings

Removal note, `2026-10-11`: the unused North Go runner, its entry/runtime helpers and asynchronous file logger were removed. These tables preserve their former configuration defaults and behavior as historical evidence. The current loader no longer exposes these fields; [current system settings](../../config/system/system.md) own the GUI's bounded live text, manual recording paths and current Summoner evidence ownership.

The old Arcane token executor accepted `left-click`/`right-click` as mouse clicks and other tokens as key presses. Unspecified token pauses used its 0.35–0.5 second range. These are historical executor details; current Summoner uses configured keyboard sequences with a 0.4-second fallback. North Go attempt recordings/summaries used `recording.directory`, whose default was `recordings`.

| Former recording field | Default | Historical behavior |
| --- | --- | --- |
| `keep_failed_runs` | `true` | Kept north-go recordings when status was neither `end` nor `end_latest_frame`, including stopped attempts. |
| `keep_successful_runs` | `false` | When true, kept every north-go attempt recording. |

The former GUI event logger used a bounded file queue and rotated segments; its log was named `north-go-tuning.log`. It was separate from generic CLI logging.

| Former logging field | Default | Historical behavior |
| --- | --- | --- |
| `directory` | `logs` | Destination for `north-go-tuning.log` and rotated segments. |
| `max_segment_size_mb` | `10` | Rotation threshold in MiB, clamped to at least 1. |
| `retained_segments` | `5` | Number of rotated segments retained in addition to the active log, clamped to at least 1. |
| `flush_interval_seconds` | `0.5` | Periodic flush interval, clamped to at least 0.1 seconds. |
| `queue_max_size` | `5000` | Pending record capacity, clamped to at least 100. When full, low-priority records could be dropped and priority 0 records were written synchronously instead. |

| Former `north_go_tuning` field | Default | Historical behavior |
| --- | --- | --- |
| `rerun_count` | `1` | North Go Test fallback count. |
| `auto_record_runs` | `true` | North-route automatic video setting. |
| `keep_reference_runs` | `1` | First-N video retention. |

These settings did not prune the later Summoner evidence directory or ordinary manual GUI recordings. Development-session cleanup remains separate from runtime retention and does not require indefinite storage of raw development artifacts.

## Waypoint Entry Maintenance

The historical entry flow lived in `routes/arcane_entry.py`, which was removed with the unused runner. The following notes describe that implementation; current room/waypoint entry belongs to [town-room-loop guidance](town-room-loop.md). Keep the minimap visible for marker search; its waypoint marker supplies direction guidance. The interaction target is the real waypoint object in the world view.

The search moves toward the town center-right anchor, then checks upward, downward, and rightward holds while watching for the waypoint. Release held movement when detection succeeds. Keep waypoint reacquisition from the current frame in the approach/click path, and review fallback coordinates after movement to prevent stale targets.

After opening the waypoint list, detect each panel and use panel-relative ratios for the Act 2 tab and Arcane Sanctuary entry. Fixed screen pixels make these clicks fragile across window layouts.

`North Go Test` owned repeated attempts and reset/entry sequencing. Diagnose historical route outcomes using the matching attempt recording and enabled file logs; the GUI log was a rolling view. Check the starting state between attempts before tuning direction changes. Former storage and rerun controls are preserved [above](#removed-runner-settings); historical steering constants and frame freshness remain in [route maintenance](../../diablo2/runs/summoner/routes/docs/north_go.md).

## Current route runtime and planner direction

The north route already uses this worker structure:

- capture thread updates the latest frame continuously
- fast vision thread extracts lightweight navigation and progress signals
- slow vision thread extracts landmark, template, and heavier recognition signals
- decision thread selects and sends route movement

In the legacy north-route workers, monster and configured loot detections delay movement without executing combat, pickup or life care. The separate continuous main releases F2 for bounded nearby F4 Nova and then replans from the current frame. It does not infer a kill from attack completion. Complete pickup, farther combat detours and route rejoining remain open under the [shared coordinator priority](run-coordination.md#priority-order).

In this model:

- route files like `north_go.py` should eventually become movement-intent providers
- the planner should decide whether movement is allowed on each tick
- the same planner model should later be reusable across Summoner, Diablo, and other runs

## Current route-control limitation

현재 제1목표는 북쪽 끝→중앙 실물 웨이포인트→Act 1→저장 후 종료→새 방→북쪽을 반복하는 감독 개발 시험이다. `--north-cycles 1..5`이 새 방 묶음을 실행하며 `--return-to-hub-from ...`을 함께 쓰면 기존 방의 복귀/종료는 준비로 분리한다. Act 1과 캐릭터 선택 화면을 확인해야 다음 방을 생성한다. 현재 최초 안전/버프 효과는 방마다 감독 확인하며, 미완료 이동이나 서모너 처치/회수 미확인에서는 다음 방을 생성하지 않는다. 빈 북쪽 끝의 개발 마감도 전체 Summoner 수용 성공과 구분한다. 동/남/서는 이 흐름이 안정된 뒤 연결한다.

복귀 중 실물 중앙을 발견하면 연속 이동을 해제하고 현재 바닥으로 단일 접근한다. 일시 미검출은 중앙 재획득 대기이며 반대 날개 출발이 아니다. 근접100px와 실제 정지 검증은 유지한다. 현재 넓은 착지 후보 상한은 지역 보정의800px이며 능력 최대 거리나 모든 지형의 통과를 뜻하지 않는다.

현재 감독 개발 경로는 `main.py --summoner-run --live --continuous-north`의 F2 연속 유지·넓은 조향이다. 기존 단일 시전 모드는 제한된 회귀 경로로 남아 있다. `--no-combat`로 근접 F4 전투 모듈을 끌 수 있으며 HUD 비율·포커스·중단·현재 화면 시한 검사는 유지한다. `--return-to-hub-from north|east|south|west`는 같은 방의 중앙 복귀 시험이고 새 방을 만들지 않는다. `--north-cycles 1..5`은 북쪽 끝→중앙→Act 1→방 종료→새 방을 연결한다. 초기 복귀 뒤 새 방 진입은 실제 통과했지만 새 방 북쪽 왕복은 아직 실패한다. `--continue-navigation <report>`는 현재 지형 대응을 확인한 뒤 국소 꺾임만 이어받는다. 현재 북쪽 반복의 끝 판독·복귀를 보정 중이며, 네 방향 실제 완주와30방 수용은 미완료다. 최신 결과는 [통합 실행 기록](../plans/run/run-20261009-07-summoner-live-integration.md#attempt8-북쪽-도달과-새-방-단위-반복-연결)이 소유한다.

연속 경로의 전체 북쪽 흐름은 화면2시 방향이며 국소 좌우 꺾임과 구분한다. `steering.py`는 기존 발판 조준이 닫히면 즉시 재조준하고, 옆길이 계속 열려 있으면 큰 발판 안쪽의 북쪽 후보만으로 옆길을 끝내지 않는다. 등각 방향 비교의 높이 보정2, 북쪽 재개 각도20°·2관찰, 국소 방향40°, 급회전35°·두 후보 일치20°는 현재 개발 보정이다. 이동 상한/착지 여유는 지역 `navigation-calibration.json`이 소유한다. 정지 조준 제외 반경은 착지 여유×2, 정지 관찰2초/최대3회이며 실제 지형 대응 후 제외 영역을 폐기한다. 일반적인 모든 계단/끝/가림 판독을 보장하지 않는다.

연속 빠른 시전은 유지 상태에서 참고 시전 간격마다 keyDown을 반복하며, 커서 변경 후50ms 처리 시간이 지난 새 관찰에서 입력한다. 단일 keyDown만으로 연속 시전까지 확인됐다고 보지 않는다. 이동 대응의 일시 끊김은 참고 시간×2 재획득 뒤에도 실패할 때만 새 국소 좌표로 바꾼다. 같은 지형 재개는 원래 실제 시전 타이머가 남고 이전 효과 확인이 유효할 때 버프를 재사용할 수 있지만 현재 효과·장비는 다시 확인한다. 갱신 시한을 연장하지 않으며 자동 안전 판독/갱신의 실게임 수용은 남아 있다.

현재 main의 검토된 카메라 배치는 최신 감독 확인 뒤 동일 프레임의 포커스·HUD·닫힌 벨트·알려진 패널 조건이 유효한 동안 가림을 보완한다. 기본 몸 보정 모드의2초 상한과 구분하며 과거 캡처를 현재 몸/버프 근거로 쓰지 않는다. 끝은 세 공간 부위와 정지로 승인하고, 최초 세 부위 확인 후 최대2초의 정적 두 부위 재확인만 별도로 허용한다. 상세 조건과 제한은 [SPEC-0012](../plans/spec/spec-0012-arcane-perceptual-navigation.md)가 소유한다.

The immediate focus is reaching the north end, returning to the real central waypoint and repeating through Act 1 and a new room. Actual progress and failures belong to the integration run; the legacy GUI route remains separate.

The current `north_go` controller uses floor/direction signals and local correction to reach the north end. Detection pauses exist, but combat/loot detours and full route recovery remain incomplete. Detailed tuning guidance lives beside the route in [north_go.md](../../diablo2/runs/summoner/routes/docs/north_go.md). Use the [route evaluation standard](../evaluation/route-evaluation.md) to compare steering behavior and recordings.

## Future interruption note

Nearby Nova already interrupts continuous main. Farther hunting and looting will also need explicit approach and route-rejoin ownership beyond the legacy route-stage logic.

Example future situation:

- moving toward `2 o'clock`
- route is interrupted by combat or wanted loot
- character temporarily turns backward or moves off the route
- after the interruption, route movement should resume and continue toward the goal

The current route controller does not establish a reliable rejoin after that case; detection delays alone do not provide detour ownership or recovery.

That means later we will need pause/resume-safe route control with concepts like:

- route active vs route paused
- interruption reason tracking
- temporary local detour ownership by combat or loot logic
- route re-acquire / route rejoin step before continuing staged movement
- path-stage advance disabled while route ownership is not active

For now, this is intentionally out of scope.

The current implementation focus remains:

- reach the goal first
- keep the route stable under uninterrupted movement
- postpone full hunting/looting interruption recovery until after basic route completion is reliable

## Remaining implementation targets

[공통 필드 제어](field-control.md)는 후속 승인 범위에서 단일 F2/F4 소유자, 최신 화면/버프 시한 해제, 포션 확인·잔량 재관찰과 연속 프로필 버프 실행기를 구현했다. 기존 북쪽 경로에서는 stale/적/아이템/쿨다운/정지 반환 때 유지 키를 해제하도록 수정했다. 자동 안전/개체 사망/획득을 생산하는 코드와 GUI 경로 소비자는 연결 전이다. [현재 실행 기록](../plans/run/run-20261005-06-field-control-loop.md)이 공통 계약 검증과 짧은 감독 입력의 증거·한계를 소유한다.

- validate route completion on screenshots, recordings, and supervised runs
- add survival/combat/loot arbitration with interruption-safe route recovery
- implement the remaining payload states, then attach normal post-run exit and repeat handling
- unify live action dry-run, pause/stop, and difficulty handling; current scope is documented in [system.md](../../config/system/system.md)


현재 감독 복귀에서 중앙 재개를 확인한 경우 `at_central_waypoint=true`로 일반 이동을 먼저 보류할 수 있다. 자동 도착은 현재 전체 중앙 근거·100px 근접·정지로 별도 확인한다. 효과와 합쳐진 불꽃은 현재 문양 두 곳과 양쪽 예상 불꽃 영역을 함께 검사하며 과거 좌표/점수를 사용하지 않는다.185의 실제 F2 최장 유지22.469초와187의 근접 전투 소비는 부분 기능 증거이며 새 방 완전 왕복/5방 완료나 처치·열쇠 획득 성공이 아니다.
