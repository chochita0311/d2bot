# Summoner Run Spec

Source recording: `recordings/diablo2_20260314_010525.avi`

## Goal

Describe the observed Summoner Run as a state-based spec that can be implemented incrementally.

## Observed timeline

- `0s-3s`: character select ready
- `3s-4s`: click `플레이`
- `4s-5s`: choose `지옥`
- `5s-8s`: loading
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
- `93s-105s`: Act 1 town again, post-run handling visible, full remake not shown before recording ends

## Lifecycle states

### 1. `character_select_ready`

Expected evidence:

- character list visible
- `플레이` button visible
- correct character row already selected

Success action:

- click `플레이`

Failure / retry:

- stop if character-select screen cannot be confirmed safely

### 2. `difficulty_select_ready`

Expected evidence:

- difficulty modal visible
- `지옥` button visible

Success action:

- click `지옥`

Failure / retry:

- if modal does not appear after play, retry with bounded attempts

### 3. `loading_after_create`

Expected evidence:

- loading panel visible

Success action:

- wait until town screen is visible

### 4. `act1_town_loaded`

Expected evidence:

- Rogue Encampment scene visible
- life and mana orbs visible
- movement/input available

Success action:

- tap `ALT` once if labels are not already enabled

### 5. `labels_enabled`

Expected evidence:

- dropped item labels stay visible without holding `ALT`

Success action:

- move to waypoint area

### 6. `waypoint_open`

Expected evidence:

- waypoint panel visible

Success action:

- switch to Act 2 tab
- click `비전의 성역`

### 7. `arcane_loaded`

Expected evidence:

- Arcane Sanctuary area name or recognizable geometry visible

Success action:

- commit to one wing and start search

### 8. `wing_search_started`

Expected evidence:

- movement and combat along one wing

Success action:

- clear blockers only
- continue until Summoner platform is found

### 9. `summoner_detected`

Expected evidence:

- `소환사` / Summoner platform visible
- boss name visible or platform geometry strongly confirmed

Success action:

- kill target

### 10. `summoner_killed`

Expected evidence:

- target dead
- post-kill item labels visible

Success action:

- run loot scan for `key of hate`

### 11. `loot_scan_complete`

Expected evidence:

- keep-or-ignore decision made for visible drops

Success action:

- if no key and journal available, click `호라존의 일지`

### 12. `journal_clicked`

Expected evidence:

- journal interacted with
- red portal available

Success action:

- enter portal

### 13. `canyon_loaded`

Expected evidence:

- Canyon / red portal destination loaded

Success action:

- return to town and prepare for remake

### 14. `post_run_complete`

Expected evidence:

- back in town with no further target actions pending

Success action:

- leave game
- return to character select
- start next run if repeat budget remains

## Execution shape

The future Summoner Run should be arranged as three layers:

- front half: create room and enter the game safely
- payload: perform the actual Summoner job
- back half: leave the game and return to character select

The reusable room-half logic should live in `run_lifecycle` and be imported by future runs such as Summoner Run or Diablo Run.

That means the long-term flow should read like:

- make room via `run_lifecycle.create_room(...)`
- run Summoner job in the farm module
- exit room via `run_lifecycle.exit_room(...)`

Inside the Summoner payload, the job can then be split again into:

- hunt
- loot
- act and waypoint movement
- journal and portal handling
- town return and finish

## First implementation target

Build only the room lifecycle first:

- `character_select_ready`
- `difficulty_select_ready`
- `loading_after_create`
- `act1_town_loaded`
- leave game / return to character select
- repeat until count is exhausted or user interrupts

This gives a safe loop for testing room creation and remake before waypointing or combat is added.

## Profile resolution

The Summoner payload should not depend on `run_profiles` having the active selection embedded inside it.

The Summoner module should resolve its own required profile id, `summoner`, directly from `run_profiles`.

That keeps the run catalog reusable while still letting the Summoner payload pull its own hunting, loot, life, and run-specific pieces together without a shared global run selection.

## Shared coordinator reference

Use [run_coordination.md](run_coordination.md) for the general live coordination model that combines hunting, combat, loot watching, and safety priority during a run.

