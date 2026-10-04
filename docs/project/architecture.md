# Architecture

This document organizes the high-level product direction and system structure for the Diablo 2 Windows GUI automation project.

## Current implementation

| Area | Responsibility |
| --- | --- |
| `main.py`, `diablo2/app.py` | CLI arguments and dispatch to the desktop GUI or CLI preview. |
| `diablo2/ui/gui.py` | Tkinter capture/recording controls, character-config selection, and action launch/stop controls. |
| `diablo2/actions/` | Recording, gem combining, ground-template loot pickup, and room create/exit sessions. |
| `diablo2/runs/` | Run definitions and the staged Summoner orchestrator; route pieces and runtime helpers live under `runs/summoner/`. |
| `diablo2/common/` | Config loading, Windows capture, template detection, movement input, real-time vision workers, CLI controller, and asynchronous logs. |
| `diablo2/core/bot.py` | Older CLI capture/template/overlay loop. |
| `config/`, `assets/` | Runtime settings and profiles, plus screenshot-derived matching assets. |

The current Summoner stages are documented in [summoner-run.md](../features/summoner-run.md). The north route uses capture, fast/slow vision, and decision workers; detection can delay route decisions, but a complete survival/combat/loot coordinator is still a target.

The CLI controller owns configurable dry-run, pause, and stop behavior. GUI action sessions send live input directly and have separate interruption handling; shared safety and pause coverage remain incomplete. See [system settings](../../config/system/system.md) for the actual setting and hotkey scope.

The sections below describe product requirements and future extensions unless explicitly marked as current.

## Product direction

The app should operate through normal Windows GUI interaction only:

- observe the screen
- understand game state from images and text
- decide what to do from configurable rules
- send normal mouse and keyboard input when enabled

Current observation tools include CLI dry-run/overlays, logging, recording, and snapshots. Further support should include:

- shared dry-run behavior across action sessions
- replay analysis

## Core system areas

### 1. Control surface

The Tkinter desktop GUI is the current operator-facing control layer; the CLI preview remains available.

Further controls:

- start bot profile
- pause and resume
- stop immediately
- enable dry-run vs live input
- switch between farm profiles

Possible future surfaces:

- tray icon
- small overlay status panel

### 2. Human override and interruption

Manual user input must take priority over automation.

Required behavior:

- if the user presses the configured stop hotkey, the bot stops
- if the user presses the configured pause hotkey, the bot pauses
- if the user starts actively using mouse or keyboard, automation should suspend or stop based on configuration
- bot actions should always be recoverable without restarting the program

Implementation ideas:

- global hotkeys
- recent-user-input watcher
- cooldown period before automation resumes

### 3. Survival logic

The bot should protect the character before trying to optimize farming.

Required signals:

- life orb or life bar monitoring
- mana monitoring
- potion belt availability
- mercenary survival status if relevant
- dangerous-state detection such as death screen, low life, frozen screen, or disconnect-like states

Required actions:

- drink life potion
- drink mana potion
- use rejuvenation potion
- retreat or town portal in critical state
- stop automation if survival logic is uncertain

### 4. Character behavior

Combat behavior should depend on the character build.

Current character configs provide movement keys and buff action sequences. The GUI selects these configs; automatic in-game character-row selection and build-aware combat remain future work. Field details live in [characters.md](../../config/characters/characters.md).

Examples:

- hammerdin
- blizzard sorceress
- lightning sorceress
- javazon
- summon necromancer

Behavior inputs:

- primary attack skill
- secondary attack skill
- buff cycle
- movement style
- potion thresholds
- targeting rules

### 5. Game mode and character grouping

The app should separate shared repeatable behavior from character-specific behavior.

Shared screen/action services own repeatable UI transitions; character profiles supply bindings and overrides. Character-row detection and mode-marker design belong to [game-modes.md](../features/game-modes.md); configured fields belong to [characters.md](../../config/characters/characters.md).

### 6. Farm profiles

Automation should be organized by farm payload/profile instead of one giant script.

Early profile candidates:

- Diablo run
- Baal run
- Terror Zone farm
- Summoner run
- Pindleskin run
- Travincal run

Each profile should eventually define:

- entry conditions
- route steps
- combat rules
- loot rules
- exit conditions
- recovery rules

### 7. Loot intelligence

Loot handling should be data-driven, not buried inside code.

The current pickup session matches configured ground-item templates and clicks their labels. OCR, affix evaluation, and a survival-aware approach/pickup coordinator remain future work.

Needed features:

- item label OCR
- keep or ignore decision rules
- item category rules
- character/profile-specific keep rules
- logging of dropped and kept items

Data sources to use as references:

- Blizzard classic item reference: [The Arreat Summit item pages](https://classic.battle.net/diablo2exp/items/)

Suggested internal categories:

- runes
- keys
- gems
- charms
- jewels
- bases
- uniques
- sets
- crafting materials
- gold
- consumables
