# Roadmap

This document holds the current build order, open questions, and near-term upgrades.

## Established foundation

- Tkinter desktop controls are the primary interface; the older CLI preview remains available.
- Recursive JSON loading separates system settings, character actions, shared loot, and run profiles.
- Named-window capture, recordings, snapshots, template actions, and room create/exit sessions are implemented.
- Summoner is the first seeded run profile. Its single GUI action now uses current main for room entry, automatic buffs, north navigation, bounded nearby Nova, conditional loot scans and central-waypoint return. The separate North Go Test and old route engine have been removed.
- Character movement/buff configs and current-screen terrain, background, hub and terminal observations are implemented.
- Input-free [character survival state](../plans/feature/feat-0003-character-survival-state.md), [class casting reference lookup](../plans/feature/feat-0004-class-casting-rules.md), bounded [HUD/belt observation](../features/survival-observation.md), [buff confirmation](../features/field-buffs.md) and [shared field control](../features/field-control.md) are implemented. [RUN-20261005-06](../plans/run/run-20261005-06-field-control-loop.md) owns earlier API/probe evidence. Current main composes shared input pieces and its own producers; independent whole-scene clear, linked target death/item acquisition and complete `FieldRuntime` composition remain incomplete.

These pieces do not yet establish complete farming or live acceptance. Fresh-room north completion remains 0/5 and accepted full rooms remain zero. Current behavior is documented in [summoner-run.md](../features/summoner-run.md); the [integration handoff](../plans/run/run-20261009-07-summoner-live-integration.md#attempt15-세션-마감-정리와-다음-gui-실행-인계) owns the ordered next-session checks and actual outcomes.

The [Summoner completion experiment plan](../features/summoner-experiment-plan.md) records the approved complete-run requirements for online softcore Resurrection Flash while retaining explicitly proposed sample budgets. The approved bounded supervised [town room loop](../features/town-room-loop.md) has completed basic creation, Act 2 town visit, Act 1 return, exit, and recreation checks; its [run record](../plans/run/run-20261004-02-supervised-town-room-loop.md) owns observed results, session cleanup, and remaining obstacle/NPC/interruption verification. Arcane entry and bounded field behaviors are connected in current main; fresh-room completion, actual death/acquisition, timely automatic renewal, four-wing acceptance and complete runs remain unfinished. Act 1 maintenance details are explicitly skipped for now; the package refactor remains unapproved.

## Remaining build order

### Phase 1: Safe foundation

- validate capture and action bounds across supported window layouts
- extend CLI dry-run and pause/stop behavior to every live action path
- verify the connected Summoner difficulty, stop/focus handling and held-input cleanup across room transitions; resumable pause remains unfinished
- validate existing logs, recording retention, and user-interruption handling
- define and validate [real-time/recording isolation and bounded retention](../plans/prd/prd-0008-realtime-runtime-and-recording-retention.md) before teleport-heavy or long-run expansion; the current town loop remains unchanged
- extend existing character-select screen detection to character-row scanning

### Phase 2: Vision and OCR

- shared north encounter candidates/terrain/radius/life-evidence contracts and assets are implemented; multi-pose tracking, live HUD uncertainty handling and linked death/acquisition confirmation remain required before treating bounded nearby Nova as verified combat ([earlier evidence](../plans/run/run-20261005-05-arcane-north-encounters.md), [current integration](../plans/run/run-20261009-07-summoner-live-integration.md))
- detect core UI states
- extend reviewed life/mana observation to other layouts and combat conditions
- add OCR for item labels
- add configurable loot whitelist and ignore list
- classify character rows by mode markers

### Phase 3: Action engine

- extend the current screen-based action engine with verified recovery results
- add safety timing and retries
- add stop-on-uncertainty behavior
- support selecting a specific character row by config

### Phase 4: Character profiles

- add combat sequences
- tune survival thresholds
- allow per-character overrides on top of shared run profiles

### Phase 5: Farm profiles

- implement one farm route end-to-end
- validate recovery logic
- add loot and stash flow
- expand to other routes

## Open questions

- which Diablo 2 version and resolution will be the standard target
- how aggressive the manual-input interruption should be
- which combat build should be validated next beyond current bounded nearby Nova
- what screenshot or replay evidence will establish end-to-end run acceptance

## Near-term upgrades

- validate the implemented Act 1 waypoint/Act 2 Arcane entry and north-route tracking across supported layouts
- build a reusable hunting engine that consumes `hunting` rules instead of hardcoded path logic
- add OCR or label detection for real loot decisions beyond fixed-item template matches
- expand life/mana and belt observation beyond the reviewed town layout and connect verified observations to survival logic
- validate current automatic nearby/resource checks, belt recovery and S+140-second rebuffing, and complete independent effect/landing/acquisition confirmation; the full [common field runtime](../features/field-control.md) adapter remains separate
- complete positive Summoner death/key acquisition confirmation, then verify north end→central waypoint→Act 1→exit in a fresh room before a five-room bundle; optimize observed failures and processing time before east/south/west and the 27/30 acceptance test
- add GUI controls for run-profile selection and shared dry-run/pause behavior
