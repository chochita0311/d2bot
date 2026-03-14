# runs.json

Purpose: choose the live run and define reusable run-specific behavior.

## Top-level fields

- `run_profiles`: named run definitions.

Payload modules such as Summoner Run should resolve the run profile they need directly from this catalog.

## run_profiles.<name>

- `name`: internal profile id.
- `goal`: short summary of the run.
- `description`: longer explanation.
- `hunting`: target, waypoint, route, and combat intent.
- `loot`: run-level loot preferences on top of shared fixed items.
- `life`: potion, retreat, and safety thresholds.
- `run_specific_rules`: assumptions that belong only to this run.
- `templates`: extra run-only templates, such as boss or loot visuals.

## hunting

Use for navigation and combat intent.

Examples:

- `waypoint_act`
- `waypoint_name`
- `target_monsters`
- `target_areas`
- `route_notes`
- `fight_style`
- `search_timeout_seconds`

## loot

Use for run-local rules, not the global fixed-item list.

Examples:

- `keep_labels`
- `ignore_labels`
- `free_inventory_slots_min`

## life

Use for survival thresholds.

Examples:

- `use_healing_potion_below`
- `use_rejuvenation_below`
- `emergency_retreat_below`
- `use_mana_potion_below`

## Example

Use `run_profiles.summoner` when testing the Summoner flow.
