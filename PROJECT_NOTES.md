# Project Notes

This file organizes early ideas for the Diablo 2 Windows GUI automation project.

## Product direction

The app should operate through normal Windows GUI interaction only:

- observe the screen
- understand game state from images and text
- decide what to do from configurable rules
- send normal mouse and keyboard input when enabled

The app should be useful even before full automation by supporting:

- dry-run mode
- overlays
- logging
- session recording
- replay analysis

## Core system areas

### 1. Control surface

The program needs a simple operator-facing control layer.

Planned controls:

- start bot profile
- pause and resume
- stop immediately
- enable dry-run vs live input
- enable recording
- switch between farm profiles

Possible future UI:

- lightweight desktop GUI
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

Shared repeatable behavior examples:

- character-select screen detection
- switching online or offline tabs
- scanning character rows
- selecting a character from the list
- starting a run profile
- returning to town
- stash and sell loops

Character-specific behavior examples:

- skill bindings
- build-specific combat logic
- potion thresholds
- preferred route
- loot overrides

Character rows should eventually be classified by:

- progression mode: ladder or standard
- ruleset family: ROTC or Resurrection

Current visual clues from the captured character-select screen:

- ladder and standard can be checked from the green background marker on the right-upper edge of a row
- ROTC and Resurrection can be checked from the gold-background `X` marker on the right-upper edge of a row

### 6. Farm profiles

Automation should be organized by farm payload/profile instead of one giant script.

Early profile candidates:

- Diablo run
- Baal run
- Terror Zone farm
- Countess run
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

## Open questions to settle later

- which Diablo 2 version and resolution will be the standard target
- whether the app should use a desktop GUI or config-first workflow
- how aggressive the manual-input interruption should be
- how loot rules should be stored: JSON, YAML, or profile-specific files
- which first character build should be supported
- which first farm profile should be implemented
