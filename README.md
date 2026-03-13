# d2bot

Windows-first starter scaffold for watching a Diablo 2 game window, matching templates, recording sessions, and growing toward simple farming automation.

Project constraints and safety boundaries are documented in [PROJECT_POLICY.md](D:/python/d2bot/PROJECT_POLICY.md).
Working notes and roadmap are documented in [PROJECT_NOTES.md](D:/python/d2bot/PROJECT_NOTES.md).

## What it does now

- Captures the screen continuously with `mss`
- Runs OpenCV template matching on each frame
- Shows a preview overlay with match confidence
- Supports optional video recording
- Supports pause and stop hotkeys
- Uses a JSON config so you can control farm goals and loot/watch rules without rewriting code

## What it does not do yet

- OCR item names
- Route/path logic
- Inventory/stash logic
- Safe window targeting by title
- Multi-step farming behaviors

## Quick start

1. Create or activate a Windows virtual environment.
2. Install dependencies:

```powershell
pip install -r requirements.txt
```

3. Run the bot:

```powershell
python main.py --config config.example.json
```

4. Hotkeys:

- `F8` pause/resume
- `F9` stop
- `Esc` also stops from the preview window

## How to steer behavior

Edit `config.example.json`:

- `capture.region`: set a fixed part of the screen if you only want the game area
- `recording.enabled`: turn on session recording
- `farm.name`: your run profile name
- `farm.goal`: plain-language note for what this profile is trying to do
- `farm.loot_whitelist`: items you care about
- `farm.templates`: images to detect and what action to trigger

## Suggested next upgrades

1. Add OCR with PaddleOCR or Tesseract for loot labels.
2. Add a simple state machine: town, travel, fight, loot, stash.
3. Add window targeting so clicks are relative to the Diablo window only.
4. Add route-specific templates and recovery rules.
