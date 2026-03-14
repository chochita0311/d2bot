# Diablo2

Windows-first starter scaffold for watching a Diablo 2 game window, matching templates, recording sessions, and growing toward simple farming automation.

Project constraints and safety boundaries are documented in `docs/project/policy.md`.
Working notes and roadmap are documented in `docs/project/notes.md`.
Game mode notes are documented in `docs/features/game_modes.md`.
Farm profile structure is documented in `docs/features/farm_profiles.md`.
Run coordinator behavior is documented in `docs/features/run_coordination.md`.
Environment setup is documented in `docs/setup/environment.md`.

## What it does now

- Launches a small desktop GUI control panel by default
- Can record a named Diablo window to video with start and stop buttons
- Can capture a one-shot snapshot of the current Diablo window
- Can target a visible game window by title
- Can try named-window capture before falling back to desktop-region capture
- Supports the older OpenCV preview loop through `--cli`
- Uses a JSON config so you can control capture settings without rewriting code
- Supports reusable run-profile config sections for hunting, loot, life management, and farm-specific rules

## Quick start

1. Activate the local project environment:

```powershell
. .\scripts\activate-project.ps1
```

2. Install dependencies:

```powershell
python -m pip install -r requirements.txt
```

3. Launch the GUI:

```powershell
python main.py
```

4. In the GUI:

- select the Diablo window from the list, or type its title
- choose `auto`, `window`, or `screen` capture
- click `Start Recording` to begin saving video
- click `Stop Recording` to finish
- click `Capture Snapshot` for a still image

## Other commands

List visible windows:

```powershell
python main.py --list-windows
```

Run the older CLI preview loop:

```powershell
python main.py --cli --config config
```

## How to steer behavior

Edit files under `config/`. For field-by-field help, see [config/config.md](config/config.md) and the matching `*.md` guide inside each config subfolder.

- `run_profiles.<name>`: reusable run definitions that payload modules resolve directly when needed
- `run_profiles.<name>.hunting`: reusable target, waypoint, route, and combat intent
- `shared_loot.fixed_items`: fixed items that should stay on the basic keep list for all runs
- `run_profiles.<name>.loot`: run-specific keep or ignore decisions on top of the shared fixed-item list
- `run_profiles.<name>.life`: reusable potion and retreat thresholds
- `run_profiles.<name>.run_specific_rules`: assumptions that belong only to one run
- `capture.window_title`: target a window such as `Diablo II` or `Diablo II: Resurrected`
- `capture.window_title_mode`: `contains` or `exact`
- `capture.follow_window`: refresh the capture box if the game window moves
- `capture.capture_backend`: `auto`, `window`, or `screen`
- `capture.region`: set a fixed part of the screen if you only want the game area
- `recording.enabled`: used by the older CLI preview loop
- `characters`: define character-specific overrides such as mode, ruleset family, and preferred run profile

## Current seeded run profiles

- `summoner`: Arcane Sanctuary (`비전의 성역`) run for The Summoner (`소환술사`) and `key of hate`

## Suggested next upgrades

1. Add waypoint-screen detection and route-state tracking for Act 2 travel.
2. Build a reusable hunting engine that consumes `hunting` rules instead of hardcoded path logic.
3. Add OCR or label detection for real loot decisions beyond fixed-item template matches.
4. Add life and mana monitoring for survival logic.
5. Add more GUI controls for profile selection and safe automation toggles.
