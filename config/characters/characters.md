# characters.json

Purpose: character metadata and future character-specific behavior.

## game_modes

Reference values for character classification.

- `progression_modes`: examples include `ladder` and `standard`.
- `ruleset_families`: examples include `rotw` and `resurrection`.
- `detection_notes`: human notes for distinguishing character list markers. Use explicit present-versus-absent rules so ladder/standard and Resurrection/ROTW are not ambiguous.

## characters

Each entry is one known character.

Fields:

- `display_name`: readable in-game name.
- `progression_mode`: one of the progression modes.
- `ruleset_family`: one of the ruleset families.
- `preferred_run_profile`: default run profile for that character.

## Expansion direction

This file is the right place for future character-level overrides such as:

- preferred difficulty
- loot additions or removals
- buff behavior
- inventory or potion assumptions


