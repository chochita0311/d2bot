# Farm Profiles

This document defines the reusable structure for farming runs.

## Design goal

Separate farming behavior into four layers:

- hunting: how we reach and kill the target
- loot: what we keep and what we ignore for the active run, on top of shared fixed-item loot
- life: when we drink, retreat, or stop
- run-specific rules: assumptions that belong only to one farm

That keeps the hunting, loot, and life engines reusable across Summoner, Pindleskin, Diablo, and later runs.

## Shared fixed-item loot

Use `shared_loot.fixed_items` for priceless fixed items that should stay on the basic keep list regardless of run payload or character.

Examples:

- keys
- gems
- other fixed items with deterministic labels

This shared item list should drive the first reusable loot pickup flow.
Advanced rare, unique, or affix-threshold evaluation can be layered on later as a separate rule system.

## Shared model

Each run profile can define these sections.

### `hunting`

Use this for reusable combat and pathing intent.

Suggested fields:

- `objective`
- `waypoint_act`
- `waypoint_name`
- `target_monsters`
- `target_areas`
- `route_notes`
- `fight_style`
- `search_timeout_seconds`
- `disengage_on_uncertainty`

### `loot`

Use this for keep or ignore decisions.

Suggested fields:

- `keep_labels`
- `ignore_labels`
- `potion_columns_reserved`
- `free_inventory_slots_min`
- `identify_before_drop`
- `pickup_gold`

### `life`

Use this for safety and sustain rules.

Suggested fields:

- `use_healing_potion_below`
- `use_rejuvenation_below`
- `emergency_retreat_below`
- `use_mana_potion_below`
- `town_portal_on_risk`
- `stop_on_death_screen`
- `belt_restock_healing_below`
- `belt_restock_mana_below`

### `run_specific_rules`

Use this only for facts that should not leak into the shared engines.

Examples:

- boss-specific completion conditions
- profile-only exit rules
- map-specific assumptions
- one-off loot exceptions

## Summoner Run profile

The first real profile is the Summoner Run, stored under the internal profile id `summoner`.

Purpose:

- use the Act 2 waypoint system
- travel to Arcane Sanctuary (`비전의 성역`)
- locate and kill The Summoner (`소환술사`)
- keep `key of hate`
- ignore low-value loot and unnecessary full-clear behavior

### Profile-only rules

These should stay in the Summoner Run profile instead of the shared hunting or loot engine:

- Arcane Sanctuary is the target area
- The Summoner is the target monster
- the run succeeds only after the Summoner dies and the key pickup decision is made
- the run should end after approved loot is handled

## Implementation direction

General live action priority and coordination are documented in [run_coordination.md](run_coordination.md).

The code should read the active run profile and eventually hand its sections to:

- a reusable hunting engine
- a reusable loot engine
- a reusable life-management engine
- a thin farm-specific coordinator

