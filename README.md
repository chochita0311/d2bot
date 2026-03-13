# d2bot

Windows-first starter scaffold for watching a Diablo 2 game window, matching templates, recording sessions, and growing toward simple farming automation.

Project constraints and safety boundaries are documented in `docs/project/policy.md`.
Working notes and roadmap are documented in `docs/project/notes.md`.
Game mode notes are documented in `docs/features/game_modes.md`.
Environment setup is documented in `docs/setup/environment.md`.

## What it does now

- Launches a small desktop GUI control panel by default
- Can record a named Diablo window to video with start and stop buttons
- Can capture a one-shot snapshot of the current Diablo window
- Can target a visible game window by title
- Can try named-window capture before falling back to desktop-region capture
- Supports the older OpenCV preview loop through `--cli`
- Uses a JSON config so you can control capture settings without rewriting code

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
python main.py --cli --config config.json
```

## How to steer behavior

Edit `config.json`:

- `capture.window_title`: target a window such as `Diablo II` or `Diablo II: Resurrected`
- `capture.window_title_mode`: `contains` or `exact`
- `capture.follow_window`: refresh the capture box if the game window moves
- `capture.capture_backend`: `auto`, `window`, or `screen`
- `capture.region`: set a fixed part of the screen if you only want the game area
- `recording.enabled`: used by the older CLI preview loop
- `run_profiles`: define shared repeatable actions such as Diablo, Baal, or Terror Zone runs
- `characters`: define character-specific overrides such as mode, ruleset family, and preferred run profile

## Suggested next upgrades

1. Add character-row detection on the select screen.
2. Classify rows by the right-side marker icons for ladder/standard and ROTW/Resurrection.
3. Add life and mana monitoring for survival logic.
4. Add OCR with PaddleOCR or Tesseract for loot labels.
5. Add more GUI controls for profile selection and safe automation toggles.
