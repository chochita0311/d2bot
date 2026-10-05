# Roadmap

This document holds the current build order, open questions, and near-term upgrades.

## Established foundation

- Tkinter desktop controls are the primary interface; the older CLI preview remains available.
- Recursive JSON loading separates system settings, character actions, shared loot, and run profiles.
- Named-window capture, recordings, snapshots, template actions, and room create/exit sessions are implemented.
- Summoner is the first seeded run profile. Its executable stages currently end at the north-wing route goal, with a separate repeat-test path.
- Character movement/buff configs and north-route capture/vision/decision workers are implemented.

These pieces do not yet form complete farming or shared survival/interruption handling. Current stage boundaries are documented in [summoner-run.md](../features/summoner-run.md).

The [Summoner completion experiment plan](../features/summoner-experiment-plan.md) defines the broader draft scope for online softcore Resurrection Flash. The approved bounded supervised [town room loop](../features/town-room-loop.md) has completed basic creation, Act 2 town visit, Act 1 return, exit, and recreation checks; its [run record](../plans/run/run-20261004-02-supervised-town-room-loop.md) owns observed results, session cleanup, and remaining obstacle/NPC/interruption verification. Survival/buffs, Arcane entry, four-wing recovery, nearby-enemy combat, shared loot and maintenance, and complete runs remain separate stages. The package refactor remains unapproved.

## Remaining build order

### Phase 1: Safe foundation

- validate capture and action bounds across supported window layouts
- extend CLI dry-run and pause/stop behavior to every live action path
- resolve Summoner/North Go hotkey registration and difficulty propagation
- validate existing logs, recording retention, and user-interruption handling
- define and validate [real-time/recording isolation and bounded retention](../plans/prd/prd-0008-realtime-runtime-and-recording-retention.md) before teleport-heavy or long-run expansion; the current town loop remains unchanged
- extend existing character-select screen detection to character-row scanning

### Phase 2: Vision and OCR

- detect core UI states
- detect life and mana
- add OCR for item labels
- add configurable loot whitelist and ignore list
- classify character rows by mode markers

### Phase 3: Action engine

- extend the staged action engine with explicit recovery states
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
- which combat build should be validated first beyond current movement/buff bindings
- what screenshot or replay evidence will establish end-to-end run acceptance

## Near-term upgrades

- validate the implemented Act 1 waypoint/Act 2 Arcane entry and north-route tracking across supported layouts
- build a reusable hunting engine that consumes `hunting` rules instead of hardcoded path logic
- add OCR or label detection for real loot decisions beyond fixed-item template matches
- add life and mana monitoring for survival logic
- complete Summoner boss, loot decision, journal/portal, and post-run stages
- add GUI controls for run-profile selection and shared dry-run/pause behavior
