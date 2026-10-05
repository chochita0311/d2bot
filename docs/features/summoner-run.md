# Summoner Run Spec

The target payload describes the full run. Executable coverage is listed under Current staged implementation.

The [Summoner experiment plan](summoner-experiment-plan.md) maps the new draft PRDs for room repetition, Flash survival/buffs, all four wings, nearby-enemy combat, shared loot/identification, and town maintenance. Those PRDs own the proposed completion boundary; this reference retains the existing observed timeline and implementation coverage. No draft is authorization to run the game.

The separately approved [town room loop](town-room-loop.md) has finite create→Act 2 town→Act 1→exit→recreate behavior. It does not invoke the Summoner combat payload; its own run record distinguishes that runtime evidence from the staged payload below.

## Goal

Describe the observed Summoner Run as a payload-first target spec, and distinguish it from the narrower executable stages below.

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

The current executable Summoner run stage is intentionally narrower than the full payload timeline above.

Right now the orchestrated flow is:

1. `make_room`
2. `arcane_entry`
3. `enable_labels`
4. `buff_before_run`
5. `north_go`

`Summoner Run` executes this sequence once. `North Go Test` repeats the entry/route sequence, optionally records attempts, writes outcome summaries, and exits to character select between attempts. Its retention settings are documented in [system.md](../../config/system/system.md).

Reaching the route goal is not evidence that the Summoner was killed or that the key decision is complete. Combat, survival care, journal/portal interaction, and complete post-run handling remain target behavior.

## Waypoint Entry Maintenance

The entry flow lives in [arcane_entry.py](../../diablo2/runs/summoner/routes/arcane_entry.py). Keep the minimap visible for marker search; its waypoint marker supplies direction guidance. The interaction target is the real waypoint object in the world view.

The search moves toward the town center-right anchor, then checks upward, downward, and rightward holds while watching for the waypoint. Release held movement when detection succeeds. Keep waypoint reacquisition from the current frame in the approach/click path, and review fallback coordinates after movement to prevent stale targets.

After opening the waypoint list, detect each panel and use panel-relative ratios for the Act 2 tab and Arcane Sanctuary entry. Fixed screen pixels make these clicks fragile across window layouts.

`North Go Test` owns repeated attempts and reset/entry sequencing. Diagnose route outcomes using the matching attempt recording and enabled file logs; the GUI log is a rolling view. Check the starting state between attempts before tuning direction changes. Storage and rerun controls are owned by [system settings](../../config/system/system.md); steering constants and frame freshness are owned by [route maintenance](../../diablo2/runs/summoner/routes/docs/north_go.md).

## Current route runtime and planner direction

The north route already uses this worker structure:

- capture thread updates the latest frame continuously
- fast vision thread extracts lightweight navigation and progress signals
- slow vision thread extracts landmark, template, and heavier recognition signals
- decision thread selects and sends route movement

Monster and configured loot detections currently delay movement decisions. They do not execute combat, pickup, or life care inside the route. The intended next step is for the decision worker to arbitrate those concerns using the [shared coordinator priority](run-coordination.md#priority-order).

In this model:

- route files like `north_go.py` should eventually become movement-intent providers
- the planner should decide whether movement is allowed on each tick
- the same planner model should later be reusable across Summoner, Diablo, and other runs

## Current route-control limitation

For the current implementation stage, the immediate focus is only reaching the route goal reliably.

The current `north_go` controller uses floor/direction signals and local correction to reach the north end. Detection pauses exist, but combat/loot detours and full route recovery remain incomplete. Detailed tuning guidance lives beside the route in [north_go.md](../../diablo2/runs/summoner/routes/docs/north_go.md). Use the [route evaluation standard](../evaluation/route-evaluation.md) to compare steering behavior and recordings.

## Future interruption note

Later, when hunting and looting are allowed to interrupt route movement, the current route-stage logic will not be sufficient by itself.

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

- validate route completion on screenshots, recordings, and supervised runs
- add survival/combat/loot arbitration with interruption-safe route recovery
- implement the remaining payload states, then attach normal post-run exit and repeat handling
- unify live action dry-run, pause/stop, and difficulty handling; current scope is documented in [system.md](../../config/system/system.md)
