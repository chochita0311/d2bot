# Game Modes

This document organizes how the app should classify Diablo 2 character-select modes and route behavior.

## High-level idea

The program should separate behavior into two layers:

- general repeatable actions
- character-specific actions

That lets us reuse shared flows while still allowing a specific character to have its own skill rotation, potion thresholds, farm route, or loot priorities.

## Behavior model

### 1. General repeatable actions

These are reusable flows that do not belong to one character only.

Examples:

- launch game window monitoring
- detect character-select screen
- switch online or offline tab
- scan character rows
- select a character from the list
- start a run profile
- return to town
- stash or sell loop
- stop on danger or manual user interruption

### 2. Character-specific actions

These are overrides tied to a particular character.

Examples:

- skill bindings
- buff order
- potion thresholds
- combat style
- preferred farm profile
- loot keep overrides
- inventory layout assumptions

## Character-select categorization

At the character-select screen, the app should classify characters by two dimensions.

### A. Progression mode

- ladder
- standard

Planned visual signal:

- green background icon on the right-upper edge of a character row

### B. Ruleset / game family

- ROTW
- Resurrection

Planned visual signal:

- gold-background `X` icon on the right-upper edge of a character row

## Detection strategy

The app should classify the right-side character list in this order:

1. detect that the character-select screen is visible
2. detect the right-side list panel bounds
3. detect each visible character row
4. read the character name text
5. inspect the right-upper edge of the row for mode markers
6. assign row metadata such as ladder, standard, ROTW, or Resurrection

## Suggested row data model

Each detected row should eventually produce a structure like this:

- display name
- row index
- screen bounds
- progression mode: ladder or standard
- ruleset: ROTW or Resurrection
- level if readable
- class if readable
- selected state

## Suggested config model

The project should keep shared profiles separate from character overrides.

### Shared run profiles

Examples:

- diablo_run
- baal_run
- terror_zone
- countess_run

These define reusable behavior like:

- route steps
- stop conditions
- loot policies
- recovery logic

### Character profiles

Examples:

- abyss_knight
- storage
- elsa

These define per-character behavior like:

- allowed modes
- preferred run profile
- skill map
- potion thresholds
- preferred inventory assumptions

## Practical interpretation for the current screenshot

From the current character-select capture, the right-side list is clearly visible and is a good candidate for row-based detection.

That means the next stage should be:

- detect row rectangles
- crop the right-upper marker area from each row
- classify the marker color/icon style
- map the row to ladder vs standard and ROTW vs Resurrection
