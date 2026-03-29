# Summoner Run Spec

Source recording: `recordings/diablo2_20260314_010525.avi`

## Goal

Describe the observed Summoner Run as a payload-first state spec that can be implemented before room creation and room exit are wrapped around it.

## Payload boundary

For now, define only the body of the run:

- start after the room is already created and the character can act in town
- finish when the target work is done and the character is back in a safe handoff state
- keep reusable room creation and room exit outside this module

Long-term wrapper shape:

- create room via `run_lifecycle.create_room(...)`
- execute the Summoner payload
- exit room via `run_lifecycle.exit_room(...)`
- repeat until the run budget is exhausted

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

Use [run-coordination.md](run-coordination.md) for the live priority model.

Inside `wing_search_started` through `loot_scan_complete`, the payload should behave like one coordinator loop that keeps watching:

- survival and emergency actions first
- mandatory combat second
- safe fixed-item loot pickup third
- route progress and search movement fourth

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
3. `buff_before_run`
4. `north_go`

This is the current implementation target in code, even though the full long-term payload spec also includes post-Summoner work such as journal interaction, portal usage, and post-run return handling.

## Planner priority direction

During live movement and route execution, the intended planner priority is:

1. Life care
2. Monster-close detection and combat
3. Wanted item looting
4. Movement

Interpretation:

- movement should be the default fallback behavior, not the top-level brain
- if life care is needed, it should preempt everything else
- if a dangerous or close monster requires action, combat should preempt loot and route movement
- if a wanted item is visible and the situation is safe, looting can temporarily preempt movement
- if nothing higher priority is active, the route piece continues moving

## Threaded planner direction

The intended long-term live coordinator structure is:

- capture thread updates the latest frame continuously
- fast vision thread extracts lightweight navigation and threat signals
- slow vision thread extracts landmark, template, and heavier recognition signals
- decision thread arbitrates survival, combat, loot, and movement

In this model:

- route files like `north_go.py` should eventually become movement-intent providers
- the planner should decide whether movement is allowed on each tick
- the same planner model should later be reusable across Summoner, Diablo, and other runs

## Current route-control limitation

For the current implementation stage, the immediate focus is only reaching the route goal reliably.

That means the current `north_go` controller is still being optimized for uninterrupted forward progress toward the north end, not for full interruption-safe route recovery.

Two high-value short-term fixes were identified for that goal:

- require more confirmed progress before advancing path stages
- keep floor-guided correction alive in final stage movement as well

## Future interruption note

Later, when hunting and looting are allowed to interrupt route movement, the current route-stage logic will not be sufficient by itself.

Example future situation:

- moving toward `2 o'clock`
- route is interrupted by combat or wanted loot
- character temporarily turns backward or moves off the route
- after the interruption, route movement should resume and continue toward the goal

The current route controller is not yet designed for that case, because it still assumes mostly one-way forward staged progress.

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

## Current implementation target

Organize and expose the payload structure first:

- payload state list in code
- payload summary from the `summoner` run profile
- GUI visibility for the payload order and handoff boundary

After that, add the wrapper halves:

- front half: room create and game entry
- back half: safe game exit and remake loop
