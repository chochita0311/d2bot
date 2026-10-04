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
- Offers Gem Summing, Item Looting, Room Lifecycle, and staged Summoner/North Go actions
- Uses character configs for movement keys and pre-run buff sequences
- Supports the older OpenCV preview loop through `--cli`
- Loads JSON configs for capture, recording, character actions, and reusable run rules

The Summoner flow currently covers room entry and north-wing navigation; full boss combat, survival management, and post-run handling remain development work. See [Summoner Run](docs/features/summoner-run.md) for implemented stages and planned behavior.

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
| `Summoner Run` | Start at character select; creates a room, enters Arcane Sanctuary, enables labels, buffs, and runs the north route once. |
| `North Go Test` | Starts the same entry sequence and tests the north route, with optional repeated attempts and recordings. A blank `Repeat Count` uses the configured count, initially 1. |
| `Item Looting` | Matches configured ground-item templates and clicks visible labels. |
| `Start Room Lifecycle` | Creates and exits rooms from character select using `Difficulty`. A blank `Repeat Count` repeats until stopped. |

Play controls send live mouse and keyboard input independently of the CLI `dry_run` setting. Use `Stop Action` to stop an action. `F10` is wired for Gem Summing, Item Looting, and standalone Room Lifecycle; Summoner and North Go currently rely on the stop button. The Summoner/North Go room-entry code currently uses Hell regardless of the GUI difficulty selection.

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
