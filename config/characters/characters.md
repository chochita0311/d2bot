# characters.json

Purpose: character metadata, movement bindings, and pre-run buff sequences.

## game_modes

Reference values for character classification; [Game Modes](../../docs/features/game-modes.md) owns the marker and row-detection design.
These notes describe intended visual classification; the config loader does not automatically scan or select in-game character rows.

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
- `actions`: character-specific input settings described below.

The loader initially selects the first configured character. The GUI `Character` selector changes the active config and resolves its preferred run profile when available; select the matching character separately in the game.

## actions

Fields currently used by Arcane movement and buffs:

| Field | Behavior |
| --- | --- |
| `movement_skill_key` | Required movement key for the north route. Defaults to `null`. |
| `movement_travel_mode` | `hold` keeps the movement key down during travel; `press` sends individual presses. Defaults to `hold`. |
| `movement_reposition_mode` | Same input modes for repositioning. Defaults to `press`. |
| `pre_run_buff_order` | Ordered input tokens replayed before the route, including repeated tokens. `left-click` and `right-click` send mouse clicks; other tokens send key presses. Defaults to an empty list. |
| `buff_action_pause_seconds` | Extra settle time after each matching buff token, in seconds. Keys are normalized to lowercase; unspecified tokens use the shared 0.35–0.5 second settle range. Defaults to an empty mapping. |

The loader also preserves `primary_attack_skill_key`, `left_skill_key`, `interact_skill_key`, `buff_skill_keys`, `attack_pattern`, and `engagement_style`. They describe future combat/action behavior; the current Arcane sequence is driven by `pre_run_buff_order`, not by `buff_skill_keys` alone.

## Expansion direction

This file is the right place for future character-level overrides such as:

- preferred difficulty
- loot additions or removals
- inventory or potion assumptions
