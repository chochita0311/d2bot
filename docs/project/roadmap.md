# Roadmap

This document holds the current build order, open questions, and near-term upgrades.

## Recommended build order

### Phase 1: Safe foundation

- capture only the game window region
- keep dry-run as default
- improve logs and recording
- add user override detection
- detect character-select screen and row regions

### Phase 2: Vision and OCR

- detect core UI states
- detect life and mana
- add OCR for item labels
- add configurable loot whitelist and ignore list
- classify character rows by mode markers

### Phase 3: Action engine

- build a small state machine
- add simple click and key actions
- add safety timing and retries
- add stop-on-uncertainty behavior
- support selecting a specific character row by config

### Phase 4: Character profiles

- add character build configs
- map skills to hotkeys
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
- whether the app should use a desktop GUI or config-first workflow
- how aggressive the manual-input interruption should be
- how loot rules should be stored: JSON, YAML, or profile-specific files
- which first character build should be supported
- which first farm profile should be implemented

## Near-term upgrades

- add waypoint-screen detection and route-state tracking for Act 2 travel
- build a reusable hunting engine that consumes `hunting` rules instead of hardcoded path logic
- add OCR or label detection for real loot decisions beyond fixed-item template matches
- add life and mana monitoring for survival logic
- add more GUI controls for profile selection and safe automation toggles
