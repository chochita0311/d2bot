# Diablo2

Windows-first helper project for watching a Diablo II game window, recording sessions, and running early automation flows through either a desktop GUI or the older CLI preview loop.

Project constraints and safety boundaries are documented in `docs/project/policy.md`.
Architecture notes are documented in `docs/project/architecture.md`.
Roadmap and open planning items are documented in `docs/project/roadmap.md`.
Game mode notes are documented in `docs/features/game-modes.md`.
Farm profile structure is documented in `docs/features/farm-profiles.md`.
Run coordinator behavior is documented in `docs/features/run-coordination.md`.

## What it does now

- Launches a Tkinter desktop control panel by default
- Records a named Diablo window, captures snapshots, and selects a capture backend
- Offers Gem Summing, Item Looting, Room Lifecycle, and the current Summoner GUI development run
- Uses character configs for movement keys and pre-run buff sequences
- Supports the older OpenCV preview loop through `--cli`
- Loads JSON configs for capture, recording, character actions, and reusable run rules

The Summoner GUI connects room entry, automatic buffs, current-screen north navigation, bounded nearby Nova, conditional loot scans, and central-waypoint return. Fresh-room completion, positive Summoner death/key acquisition confirmation, and live acceptance remain unfinished. See [Summoner Run](docs/features/summoner-run.md) for the current behavior and limits.

## Quick start

1. Create a local virtual environment:

```powershell
python -m venv .venv
```

2. Activate it in PowerShell:

```powershell
. .\.venv\Scripts\Activate.ps1
```

3. Install dependencies:

```powershell
python -m pip install -r requirements.txt
```

4. Launch the GUI:

```powershell
python main.py
```

5. In the GUI:

- select the Diablo window from the list
- choose `auto`, `window`, or `screen` capture
- click `Start Recording` to begin saving video
- click `Stop Recording` to finish
- click `Capture Snapshot` for a still image

## Play controls

Select the matching `Character` config before starting an action. This selects skill settings and the preferred run profile; select the actual character in the game separately.

| Control | Starting point and current scope |
| --- | --- |
| `Start Gem Summing` | Open the supported gem stash and Horadric Cube first; see [Gem Summing](docs/features/gem-summing.md) for count and verification behavior. |
| `Summoner Run` | Start at character select on the main weapon set. Uses the selected character/difficulty; `Repeat Count` is 1–5, blank = 1. Enters Arcane Sanctuary, buffs automatically, and attempts north→central waypoint→Act 1→exit. An unconfirmed encounter or unfinished route pauses the attempt. |
| `Item Looting` | Matches configured ground-item templates and clicks visible labels. |
| `Start Room Lifecycle` | Creates and exits rooms from character select using `Difficulty`. A blank `Repeat Count` repeats until stopped. |
| `Live town loop (1–3 runs)` | Enable before `Start Room Lifecycle` to create a room, visit Act 2 town, return to Act 1, and exit; requires 1–3 repeats. See [town loop](docs/features/town-room-loop.md). |

Play controls send live mouse and keyboard input independently of the CLI `dry_run` setting. Use `Stop Action` or `F10` to stop. Summoner uses the latest main runtime, with no supervisor JSON or inventory/weapon-tab inspection. Its buff sequence returns to the starting main weapon set; Flash renews from the actual S request +140 seconds when current nearby-enemy and resource-ratio checks permit it. Labels stay OFF while moving and turn ON for bounded post-combat loot scans, then OFF again. The reviewed Summoner layout is Korean remastered 1922×1140 window capture with ALT configured as a show/hide toggle.

The removed `North Go Test` and old route implementation are no longer separate execution paths. Actual Summoner death/key acquisition confirmation, fresh-room north 5-run completion and the 27/30 acceptance test remain unfinished; automatic renewal and conditional ALT still require live verification. See [current execution and next steps](docs/features/summoner-run.md).

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

Edit files under `config/`. The [config guide](config/config.md) links to the field reference for each area: app/capture settings, shared loot, run rules, and character actions.

For the CLI, `dry_run` defaults to `true`, with `F8` pause and `F9` stop configured in the [system settings](config/system/system.md). Configured hunting, loot, and life rules describe intended behavior; their presence does not mean every rule has an executing engine yet.

## Current seeded run profiles

- `summoner`: Arcane Sanctuary (`비전의 성역`) run for The Summoner (`소환술사`) and `key of hate`

## For contributors

Contributor and coding-agent instructions live in `AGENTS.md`.
The imported agent workflow and its local adoption notes are indexed in [docs/agents/README.md](docs/agents/README.md).
